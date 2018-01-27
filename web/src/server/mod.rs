extern crate actix;
extern crate actix_web;
extern crate http;
extern crate futures;
extern crate mime;
extern crate num_cpus;

mod actors;

use self::futures::{
    future,
    Future
};
use self::actix_web::{
    Application,
    HttpServer,
    HttpRequest,
    HttpResponse,
    Method,
    middleware,
    AsyncResponder,
    Body
};
use self::actix_web::headers::{
    ContentEncoding
};
use self::actix_web::dev::Handler;
use self::http::Error as HttpError;
use self::http::header;

use ::utils::ResultExt;

use ::cmp;
use ::templates;

use templates::Template;

#[derive(Clone)]
struct State {
    pub db: self::actix::SyncAddress<actors::Db>
}

fn internal_error(description: String) -> HttpResponse {
    let template = templates::InternalError::new(description);
    HttpResponse::InternalServerError().content_type("text/html; charset=utf-8")
                                       .content_encoding(ContentEncoding::Auto)
                                       .body(template.render().unwrap().into_bytes())
                                       .expect("To create internal error response")
}

impl<S> Handler<S> for templates::NotFound {
    type Result = HttpResponse;

    fn handle(&mut self, _: HttpRequest<S>) -> Self::Result {
        HttpResponse::NotFound().content_type("text/html; charset=utf-8")
                                .content_encoding(ContentEncoding::Auto)
                                .body(self.render().unwrap().into_bytes())
                                .unwrap_or_else(|error| internal_error(format!("{}", error)))
    }
}

fn redirect(to: &str) -> HttpResponse {
    HttpResponse::MovedPermanenty().header("Location", to)
                                   .finish()
                                   .unwrap_or_else(|error| internal_error(format!("{}", error)))
}

fn serve_bytes<B: Into<Body>>(bytes: B, content_type: &str) -> HttpResponse {
    HttpResponse::Ok().content_type(content_type)
                      .content_encoding(ContentEncoding::Auto)
                      .body(bytes.into())
                      .unwrap_or_else(|error| internal_error(format!("{}", error)))
}

///Serves static files with max-age 1 day
fn serve_static<B: Into<Body>>(bytes: B, content_type: &str) -> HttpResponse {
    HttpResponse::Ok().content_type(content_type)
                      .content_encoding(ContentEncoding::Auto)
                      .header(header::CACHE_CONTROL, "max-age=86400")
                      .body(bytes.into())
                      .unwrap_or_else(|error| internal_error(format!("{}", error)))
}

fn index(_: HttpRequest<State>) -> HttpResponse {
    const INDEX: &'static [u8] = include_bytes!("../../static/index.html");
    serve_static(INDEX, "text/html; charset=utf-8")
}

fn app_bundle_css(_: HttpRequest<State>) -> HttpResponse {
    const CSS: &'static [u8] = include_bytes!("../../static/app.bundle.css");
    serve_static(CSS, "text/css; charset=utf-8")
}

fn app_bundle_js(_: HttpRequest<State>) -> HttpResponse {
    const JS: &'static [u8] = include_bytes!("../../static/app.bundle.js");
    serve_static(JS, "application/javascript; charset=utf-8")
}

fn search(request: HttpRequest<State>) -> Box<Future<Item=HttpResponse, Error=HttpError>> {
    let query = match request.query().get("query") {
        Some(query) => query,
        None => return Box::new(future::ok(templates::NotFound::new().handle(request.clone())))
    };

    if let Ok(id) = query.parse::<u64>() {
        return Box::new(future::ok(redirect(&format!("/vn/{}", id))));
    }

    let query = query.to_string();

    request.state().db.call_fut(actors::SearchVn(query.clone()))
                      .and_then(move |result| match result {
                          Ok(result) => {
                              let template = templates::Search::new(&query, result);
                              Ok(serve_bytes(template.render().unwrap().into_bytes(), "text/html; charset=utf-8"))
                          },
                          Err(error) => Ok(internal_error(format!("{}", error)))
                      }).or_else(|error| Ok(internal_error(format!("{}", error))))
                      .responder()
}

fn vn(request: HttpRequest<State>) -> Box<Future<Item=HttpResponse, Error=HttpError>> {
    let id: u64 = match request.match_info().query("id") {
        Ok(result) => result,
        Err(_) => return Box::new(future::ok(templates::NotFound::new().handle(request)))
    };

    request.state().db.call_fut(actors::GetVnData(id))
                      .and_then(|result| match result {
                          Ok(Some(result)) => {
                              let template = templates::Vn::new(&result.data.title, result.hooks);
                              Ok(serve_bytes(template.render().unwrap().into_bytes(), "text/html; charset=utf-8"))

                          },
                          Ok(None) => Ok(templates::NotFound::new().handle(request)),
                          Err(error) => Ok(internal_error(format!("{}", error)))
                      }).or_else(|error| Ok(internal_error(format!("{}", error))))
                      .responder()

}

fn default_headers() -> middleware::DefaultHeaders {
    middleware::DefaultHeaders::build().header(header::SERVER, "Roseline")
                                       //Security headers
                                       .header(header::X_DNS_PREFETCH_CONTROL, "off")
                                       .header(header::X_XSS_PROTECTION, "1; mode=block")
                                       .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
                                       .finish()

}

fn application(state: State) -> Application<State> {
    Application::with_state(state).middleware(middleware::Logger::default())
                                  .middleware(default_headers())
                                  .resource("/", |res| {
                                      res.method(Method::GET).f(index);
                                  })
                                  .resource("/app.bundle.css", |res| {
                                      res.method(Method::GET).f(app_bundle_css);
                                  })
                                  .resource("/app.bundle.js", |res| {
                                      res.method(Method::GET).f(app_bundle_js);
                                  })
                                  .resource("/search", |res| {
                                      res.method(Method::GET).f(search);
                                  })
                                  .resource("/vn/{id:[0-9]+}", |res| {
                                      res.method(Method::GET).f(vn);
                                  }).default_resource(|res| {
                                      res.route().h(templates::NotFound::new());
                                  })

}

pub fn start() {
    let addr = "127.0.0.1:80";
    let cpu_num = cmp::max(num_cpus::get() / 2, 1);

    info!("Start server: Threads={} | Listening={}", cpu_num, addr);

    let system = actix::System::new("web");
    let state = State {
        db: actors::Db::new(cpu_num)
    };

    HttpServer::new(move || application(state.clone())).bind(addr).expect("To bind HttpServer")
                                                       .threads(cpu_num)
                                                       .start();

    let _ = system.run();
}
