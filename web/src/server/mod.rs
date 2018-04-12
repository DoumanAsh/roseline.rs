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
    Body,
    State,
    Path,
    Query
};
use self::actix_web::server::HttpServer;
use self::actix_web::http::{
    Method,
    ContentEncoding
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

#[derive(Clone)]
struct AppState {
    pub executor: self::actix::Addr<actix::Syn, actors::exec::Executor>,
    pub db: self::actix::Addr<actix::Syn, actors::db::Db>,
}

fn redirect(to: &str) -> HttpResponse {
    HttpResponse::MovedPermanenty().header("Location", to)
                                   .finish()
}

///Serves static files with max-age 1 day
fn serve_static_w_enc<B: Into<Body>>(bytes: B, content_type: &str, encoding: ContentEncoding) -> HttpResponse {
    HttpResponse::Ok().content_type(content_type)
                      .content_encoding(encoding)
                      .header(header::CACHE_CONTROL, "public, max-age=86400")
                      .body(bytes.into())
}

fn serve_static<B: Into<Body>>(bytes: B, content_type: &str) -> HttpResponse {
    serve_static_w_enc(bytes, content_type, ContentEncoding::Auto)
}

fn app_bundle_css(_: HttpRequest<AppState>) -> HttpResponse {
    const CSS: &'static [u8] = include_bytes!("../../static/app.bundle.css");
    serve_static(CSS, "text/css; charset=utf-8")
}

fn app_bundle_js(_: HttpRequest<AppState>) -> HttpResponse {
    const JS: &'static [u8] = include_bytes!("../../static/app.bundle.js");
    serve_static(JS, "application/javascript; charset=utf-8")
}

fn roseline_png(_: HttpRequest<AppState>) -> HttpResponse {
    const IMG: &'static [u8] = include_bytes!("../../static/Roseline.png");
    serve_static_w_enc(IMG, "image/png", ContentEncoding::Identity)
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
                          })
                          .resource("/vndb", |res| {
                              res.method(Method::GET).h(templates::Index::new("/vndb/search", "Search VNDB"));
                          })
                          .resource("/app.bundle.css", |res| {
                              res.method(Method::GET).f(app_bundle_css);
                          })
                          .resource("/app.bundle.js", |res| {
                              res.method(Method::GET).f(app_bundle_js);
                          })
                          .resource("/Roseline.png", |res| {
                              res.method(Method::GET).f(roseline_png);
                          })
                          .resource("/dump.db", |res| {
                              res.method(Method::GET).f(db_dump);
                          })
                          .resource("/search", |res| {
                              res.method(Method::GET).with2(search);
                          })
                          .resource("/vndb/search", |res| {
                              res.method(Method::GET).with2(search_vndb);
                          })
                          .resource("/vn/{id:[0-9]+}", |res| {
                              res.method(Method::GET).with2(vn);
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
