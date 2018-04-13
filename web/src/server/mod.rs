extern crate actix;
extern crate actix_web;
extern crate http;
extern crate futures;
extern crate mime;
extern crate num_cpus;

extern crate actors;

use self::futures::{
    future,
    Future
};
use self::actix::{
    Actor
};
use self::actix_web::{
    App,
    HttpRequest,
    HttpResponse,
    AsyncResponder,
    State,
    Path,
    Query
};
use self::actix_web::server::HttpServer;
use self::actix_web::http::{
    Method,
};
use self::http::Error as HttpError;
use self::http::header;

use ::io;
use ::cmp;
use ::net;

use ::templates;

use templates::{
    ServeTemplate
};

mod middleware;
mod statics;

#[derive(Clone)]
struct AppState {
    pub executor: self::actix::Addr<actix::Syn, actors::exec::Executor>,
    pub db: self::actix::Addr<actix::Syn, actors::db::Db>,
}

fn not_allowed<S>(_: HttpRequest<S>) -> HttpResponse {
    HttpResponse::MethodNotAllowed().finish()
}

fn redirect(to: &str) -> HttpResponse {
    HttpResponse::MovedPermanenty().header(header::LOCATION, to)
                                   .finish()
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String
}

fn search(query: Query<SearchQuery>, state: State<AppState>) -> Box<Future<Item=HttpResponse, Error=HttpError>> {
    let SearchQuery{query} = query.into_inner();

    if let Ok(id) = query.parse::<u64>() {
        return Box::new(future::ok(redirect(&format!("/vn/{}", id))));
    }

    let query = query.to_string();

    state.db.send(actors::db::SearchVn(query.clone()))
            .and_then(move |result| match result {
                Ok(result) => {
                    let template = templates::Search::new(&query, result);
                    Ok(template.serve_ok())
                },
                Err(error) => Ok(templates::InternalError::new(error).response()),
            }).or_else(|error| Ok(templates::InternalError::new(error).response()))
            .responder()
}

fn search_vndb(query: Query<SearchQuery>, state: State<AppState>) -> Box<Future<Item=HttpResponse, Error=HttpError>> {
    let SearchQuery{query} = query.into_inner();
    if let Ok(id) = query.parse::<u64>() {
        return Box::new(future::ok(redirect(&format!("/vndb/vn/{}", id))));
    }

    let query = query.to_string();

    let search = actors::exec::SearchVn::new(query.clone());
    let search = state.executor.send(search);
    search.and_then(move |result| match result {
        Ok(vns) => {
            let template = templates::VndbSearch::new(&query, &vns);
            Ok(template.serve_ok())
        },
        Err(error) => Ok(templates::InternalError::new(error).response()),
    }).or_else(|error| Ok(templates::InternalError::new(error).response()))
    .responder()
}

fn vn(path: Path<u64>, state: State<AppState>) -> Box<Future<Item=HttpResponse, Error=HttpError>> {
    let id = path.into_inner();

    state.db.send(actors::db::GetVnData(id))
            .and_then(|result| match result {
                Ok(Some(result)) => {
                    let template = templates::Vn::new(&result.data.title, result.hooks);
                    Ok(template.serve_ok())
                },
                Ok(None) => Ok(templates::NotFound::new().response()),
                Err(error) => Ok(templates::InternalError::new(error).response()),
            }).or_else(|error| Ok(templates::InternalError::new(error).response()))
            .responder()
}

fn db_dump(_: HttpRequest<AppState>) -> actix_web::Either<actix_web::fs::NamedFile, templates::InternalError<io::Error>> {
    extern crate db;
    use self::actix_web::fs::NamedFile;

    match NamedFile::open(db::PATH) {
        Ok(file) => actix_web::Either::A(file),
        Err(error) => {
            error!("Unable to open DB: {}. Error: {}", db::PATH, error);
            actix_web::Either::B(templates::InternalError::new(error))
        }
    }
}

fn application(state: AppState) -> App<AppState> {
    App::with_state(state).middleware(middleware::Logger::default())
                          .middleware(middleware::DefaultHeaders)
                          .middleware(middleware::normalizer::RemoveTrailingSlach::new())
                          .resource("/", |res| {
                              res.method(Method::GET).h(templates::Index::new("/search", "Search AGTH Hook"));
                              res.route().f(not_allowed);
                          })
                          .resource("/vndb", |res| {
                              res.method(Method::GET).h(templates::Index::new("/vndb/search", "Search VNDB"));
                              res.route().f(not_allowed);
                          })
                          .resource("/app.bundle.css", |res| {
                              res.method(Method::GET).f(statics::app_bundle_css);
                              res.route().f(not_allowed);
                          })
                          .resource("/app.bundle.js", |res| {
                              res.method(Method::GET).f(statics::app_bundle_js);
                              res.route().f(not_allowed);
                          })
                          .resource("/Roseline.png", |res| {
                              res.method(Method::GET).f(statics::roseline_png);
                              res.route().f(not_allowed);
                          })
                          .resource("/favicon.png", |res| {
                              res.method(Method::GET).f(statics::favicon);
                              res.route().f(not_allowed);
                          })
                          .resource("/dump.db", |res| {
                              res.method(Method::GET).f(db_dump);
                              res.route().f(not_allowed);
                          })
                          .resource("/search", |res| {
                              res.method(Method::GET).with2(search);
                              res.route().f(not_allowed);
                          })
                          .resource("/vndb/search", |res| {
                              res.method(Method::GET).with2(search_vndb);
                              res.route().f(not_allowed);
                          })
                          .resource("/vn/{id:[0-9]+}", |res| {
                              res.method(Method::GET).with2(vn);
                              res.route().f(not_allowed);
                          }).default_resource(|res| {
                              res.route().h(templates::NotFound::new());
                          })

}

pub fn start() {
    let addr = net::SocketAddrV4::new(net::Ipv4Addr::new(0, 0, 0, 0), 8080);
    let cpu_num = cmp::max(num_cpus::get() / 2, 1);

    info!("Start server: Threads={} | Listening={}", cpu_num, addr);

    let system = actix::System::new("web");

    let executor = actors::exec::Executor::default_threads(cpu_num);
    let db = executor.db.clone();
    let executor = executor.start();

    let state = AppState {
        executor,
        db,
    };
    HttpServer::new(move || application(state.clone())).bind(addr).expect("To bind HttpServer")
                                                       .threads(cpu_num)
                                                       .start();

    let _ = system.run();
}
