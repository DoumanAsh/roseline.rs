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
    Query,
    Form
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
mod error_rsp;

use self::error_rsp::ClientError;

type FutureHttpResponse = Box<Future<Item=HttpResponse, Error=HttpError>>;

#[derive(Clone)]
struct AppState {
    pub executor: self::actix::Addr<actix::Syn, actors::exec::Executor>,
    pub db: self::actix::Addr<actix::Syn, actors::db::Db>,
}

fn not_allowed<S>(_: HttpRequest<S>) -> HttpResponse {
    HttpResponse::MethodNotAllowed().finish()
}

fn redirect(to: &str) -> HttpResponse {
    HttpResponse::Found().header(header::LOCATION, to)
                         .finish()
}

fn redirect_post(to: &str) -> HttpResponse {
    HttpResponse::SeeOther().header(header::LOCATION, to)
                            .finish()
}

#[derive(Deserialize)]
struct SearchQuery {
    query: String
}

fn search((query, state): (Query<SearchQuery>, State<AppState>)) -> FutureHttpResponse {
    let SearchQuery{query} = query.into_inner();

    if let Ok(id) = query.parse::<u64>() {
        return Box::new(future::ok(redirect(&format!("/vn/{}", id))));
    }

    let query = query.trim().to_string();

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

fn search_vndb((query, state): (Query<SearchQuery>, State<AppState>)) -> FutureHttpResponse {
    let SearchQuery{query} = query.into_inner();
    let query = query.trim().to_string();

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

fn vn((path, state):  (Path<u64>, State<AppState>)) -> FutureHttpResponse {
    let id = path.into_inner();

    state.db.send(actors::db::GetVnData(id))
            .and_then(|result| match result {
                Ok(Some(result)) => {
                    let template = templates::Vn::new(result.data.id as u64, &result.data.title, result.hooks);
                    Ok(template.serve_ok())
                },
                Ok(None) => Ok(templates::NotFound::new().response()),
                Err(error) => Ok(templates::InternalError::new(error).response()),
            }).or_else(|error| Ok(templates::InternalError::new(error).response()))
            .responder()
}

#[derive(Deserialize)]
struct AddHook {
    id: u64,
    title: String,
    version: Option<String>,
    code: Option<String>
}

fn add_hook_get(query: Query<AddHook>) -> HttpResponse {
    let mut template = templates::AddHook::new(query.id, &query.title);
    template.version = query.version.as_ref().map(|version| version.as_str());
    template.code = query.code.as_ref().map(|version| version.as_str());
    template.serve_ok()
}

fn add_hook_post((query, state): (Form<AddHook>, State<AppState>)) -> FutureHttpResponse {
    let AddHook{id, title, version, code} = query.into_inner();

    let version = match version {
        Some(version) => version,
        None => return Box::new(future::ok(ClientError::new("Missing version field").into())),
    };
    let code = match code {
        Some(code) => code,
        None => return Box::new(future::ok(ClientError::new("Missing code field").into())),
    };

    let title = title.trim().to_string();
    let put_vn = actors::db::PutVn {
        id,
        title
    };

    let db = state.db.clone();
    state.db.send(put_vn).then(move |result| -> FutureHttpResponse { match result {
        Ok(Ok(vn)) => {
            let put_hook = actors::db::PutHook {
                vn,
                version: version.trim().to_string(),
                code: code.trim().to_string()
            };

            let put_hook = db.send(put_hook).then(|result| match result {
                Ok(Ok(hook)) => Ok(redirect_post(&format!("/vn/{}", hook.vn_id))),
                Ok(Err(error)) => Ok(templates::InternalError::new(error).response()),
                Err(error) => Ok(templates::InternalError::new(error).response())
            });

            Box::new(put_hook)
        },
        Ok(Err(error)) => Box::new(future::ok(templates::InternalError::new(error).response())),
        Err(error) => Box::new(future::ok(templates::InternalError::new(error).response()))
    }}).responder()
}

fn remove_hook(id: u64, version: Option<String>, state: State<AppState>) -> FutureHttpResponse {
    let version = match version {
        Some(version) => version,
        None => return Box::new(future::ok(ClientError::new("Missing version field").into())),
    };

    let get_vn = actors::db::GetVn(id);
    state.db.send(get_vn).then(move |result| -> FutureHttpResponse { match result {
        Ok(Ok(Some(vn))) => {
            let del_hook = actors::db::DelHook {
                vn,
                version
            };

            let del_hook = state.db.send(del_hook).then(move |result| match result {
                Ok(Ok(0)) => Ok(templates::NotFound::new().response()),
                Ok(Ok(_)) => Ok(redirect_post(&format!("/vn/{}", id))),
                Ok(Err(error)) => Ok(templates::InternalError::new(error).response()),
                Err(error) => Ok(templates::InternalError::new(error).response())
            });

            Box::new(del_hook)
        },
        Ok(Ok(None)) => Box::new(future::ok(templates::NotFound::new().response())),
        Ok(Err(error)) => Box::new(future::ok(templates::InternalError::new(error).response())),
        Err(error) => Box::new(future::ok(templates::InternalError::new(error).response()))
    }}).responder()
}

fn remove_hook_get((query, state): (Query<AddHook>, State<AppState>)) -> FutureHttpResponse {
    let query = query.into_inner();
    let id = query.id;
    let version = query.version;

    remove_hook(id, version, state)
}

fn remove_hook_del((query, state): (Form<AddHook>, State<AppState>)) -> FutureHttpResponse {
    let query = query.into_inner();
    let id = query.id;
    let version = query.version;

    remove_hook(id, version, state)
}

fn db_dump(_: HttpRequest<AppState>) -> actix_web::Either<actix_web::fs::NamedFile, templates::InternalError<io::Error>> {
    extern crate db;
    use self::actix_web::fs::NamedFile;
    use self::actix_web::http::ContentEncoding;

    match NamedFile::open(db::PATH) {
        Ok(file) => actix_web::Either::A(file.set_content_encoding(ContentEncoding::Auto)),
        Err(error) => {
            error!("Unable to open DB: {}. Error: {}", db::PATH, error);
            actix_web::Either::B(templates::InternalError::new(error))
        }
    }
}

fn application(state: AppState) -> App<AppState> {
    App::with_state(state).middleware(middleware::Logger::default())
                          .middleware(middleware::DefaultHeaders)
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
                          }).resource("/search", |res| {
                              res.method(Method::GET).with(search);
                              res.route().f(not_allowed);
                          })
                          .resource("/vndb/search", |res| {
                              res.method(Method::GET).with(search_vndb);
                              res.route().f(not_allowed);
                          })
                          .resource("/add_hook", |res| {
                              res.method(Method::GET).with(add_hook_get);
                              res.method(Method::POST).with(add_hook_post);
                              res.route().f(not_allowed);
                          })
                          .resource("/remove_hook", |res| {
                              res.method(Method::GET).with(remove_hook_get);
                              res.method(Method::DELETE).with(remove_hook_del);
                              res.route().f(not_allowed);
                          })
                          .resource("/vn/{id:[0-9]+}", |res| {
                              res.method(Method::GET).with(vn);
                              res.route().f(not_allowed);
                          }).resource("/about", |res| {
                              res.method(Method::GET).h(templates::About::new());
                              res.route().f(not_allowed);
                          }).scope("/download", |scope| {
                              scope.resource("/ITHVNR.zip", |res| {
                                  res.method(Method::GET).f(statics::ith_vnr);
                                  res.route().f(not_allowed);
                              }).resource("/dump.db", |res| {
                                  res.method(Method::GET).f(db_dump);
                                  res.route().f(not_allowed);
                              }).default_resource(|res| {
                                  res.route().h(templates::NotFound::new());
                              })
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
                                                       .workers(cpu_num)
                                                       .shutdown_timeout(5)
                                                       .start();

    let _ = system.run();
}
