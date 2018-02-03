extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_tls;
extern crate native_tls;
extern crate trust_dns_resolver;
extern crate actix;
extern crate vndb;

use self::futures::Future;
use self::futures::unsync::oneshot;
use self::native_tls::{TlsConnector};
use self::tokio_tls::{TlsConnectorExt, TlsStream};
use self::trust_dns_resolver::ResolverFuture;
use self::trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use self::tokio_core::net::TcpStream;
use self::tokio_io::AsyncRead;
use self::actix::prelude::*;
use self::vndb::protocol;

use ::collections::VecDeque;
use ::time;
use ::io;
use ::net;

pub struct Vndb {
    cell: Option<FramedCell<Vndb>>,
    queue: VecDeque<oneshot::Sender<io::Result<protocol::message::Response>>>
}

impl Vndb {
    pub fn new() -> Self {
        Self {
            cell: None,
            queue: VecDeque::with_capacity(10)
        }
    }
}

impl Actor for Vndb {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        const API_DOMAIN: &'static str = "api.vndb.org";
        const API_PORT: u16 = 19535;
        const TIMEOUT: u64 = 1000;

        let tls_ctx = match TlsConnector::builder() {
            Ok(builder) => match builder.build() {
                Ok(ctx) => ctx,
                Err(error) => {
                    error!("Unable to create TLS context. Error: {}", error);
                    ctx.run_later(time::Duration::new(TIMEOUT, 0), |_, ctx| ctx.stop());
                    return
                }
            },
            Err(error) => {
                error!("Unable to create TLS builder. Error: {}", error);
                ctx.run_later(time::Duration::new(TIMEOUT, 0), |_, ctx| ctx.stop());
                return
            }
        };

        let resolver = match ResolverFuture::from_system_conf(Arbiter::handle()) {
            Ok(resolver) => resolver,
            Err(error) => {
                warn!("Unable to crate system DNS resolver. Error: {}", error);
                ResolverFuture::new(ResolverConfig::default(), ResolverOpts::default(), Arbiter::handle())
            }
        };

        //into_actor() produces actix's future whose wait() method returns ()
        //so all handling should be done inside combinators.
        resolver.lookup_ip(API_DOMAIN).into_actor(self).map_err(|error, _act, ctx| {
            error!("VNDB: Unable to resolve address. Error: {}", error);
            ctx.run_later(time::Duration::new(TIMEOUT, 0), |_, ctx| ctx.stop());
        }).and_then(|ips, act, _ctx| {
            let ip = ips.iter().next().unwrap();
            let addr = net::SocketAddr::new(ip, API_PORT);
            info!("VNDB: Connecting...");
            TcpStream::connect(&addr, Arbiter::handle()).into_actor(act).map_err(|error, _act, ctx| {
                error!("VNDB: Unable to connect. Error: {}", error);
                ctx.run_later(time::Duration::new(TIMEOUT, 0), |_, ctx| ctx.stop());
            })
        }).and_then(move |socket, act, _ctx| {
            info!("VNDB: Connected over TCP.");
            tls_ctx.connect_async(API_DOMAIN, socket).into_actor(act).map_err(|error, _act, ctx| {
                error!("VNDB: Unable to perform TLS handshake. Error: {}", error);
                ctx.run_later(time::Duration::new(TIMEOUT, 0), |_, ctx| ctx.stop());
            })
        }).map(|socket, act, ctx| {
            info!("VNDB: Connected over TLS.");
            let mut cell = act.add_framed(socket.framed(protocol::Codec), ctx);
            cell.send(protocol::message::request::Login::new(None, None).into());
            act.cell = Some(cell);
        }).wait(ctx);
    }
}

impl Supervised for Vndb {
    fn restarting(&mut self, _: &mut Self::Context) {
        self.cell.take();
        for tx in self.queue.drain(..) {
            let _ = tx.send(Err(io::Error::new(io::ErrorKind::ConnectionAborted, "Restart")));
        }
    }
}


impl FramedActor for Vndb {
    type Io = TlsStream<TcpStream>;
    type Codec = protocol::Codec;

    fn closed(&mut self, error: Option<io::Error>, _: &mut Self::Context) {
        self.cell.take();
        match error {
            Some(error) => warn!("VNDB: connection is closed. Error: {}", error),
            None => warn!("VNDB: connection is closed"),
        }
    }

    fn handle(&mut self, msg: io::Result<protocol::message::Response>, _ctx: &mut Self::Context) {
        trace!("VNDB: receive {:?}", msg);
        match self.queue.pop_front() {
            Some(tx) => {
                let _ = tx.send(msg);
            },
            None => {
                match msg {
                    //As we only use Get methods, OK can be received on login only.
                    Ok(protocol::message::Response::Ok) => (),
                    msg => warn!("Received message while there was no request. Message={:?}", msg)
                }
            }
        }
    }
}

pub use self::protocol::message::request::get::{Type, Flags, Filters, Options};
pub use self::protocol::message::Response;

//We cannot implement 3pp trait on 3pp struct... :(
#[derive(Clone)]
pub struct Request(protocol::message::Request);

#[derive(Clone)]
pub struct Get {
    inner: protocol::message::request::Get
}

impl Get {
    pub fn new(kind: Type, flags: Flags, filters: Filters, options: Option<Options>) -> Self {
        Self {
            inner: protocol::message::request::Get {
                kind,
                flags,
                filters,
                options
            }
        }
    }

    pub fn vn_by_title(title: &str) -> Self {
        let filters = Filters::new().filter(format_args!("title ~ \"{}\"", title))
                                    .or(format_args!("original ~ \"{}\"", title));

        Self::new(Type::vn(), Flags::new().basic(), filters, None)
    }

    pub fn set_options(mut self, options: Option<Options>) -> Self {
        self.inner.options = options;
        self
    }
}

impl Into<Request> for Get {
    fn into(self) -> Request {
        Request(self.inner.into())
    }
}

impl Into<Request> for protocol::message::request::Login {
    fn into(self) -> Request {
        Request(self.into())
    }
}

impl ResponseType for Request {
    type Item = protocol::message::Response;
    type Error = io::Error;
}

impl Handler<Request> for Vndb {
    type Result = ResponseFuture<Self, Request>;

    fn handle(&mut self, msg: Request, _: &mut Self::Context) -> Self::Result {
        trace!("VNDB: send {}", &msg.0);

        let (tx, rx) = oneshot::channel();
        if let Some(ref mut cell) = self.cell {
            self.queue.push_back(tx);
            cell.send(msg.0.into());
        } else {
            let _ = tx.send(Err(io::Error::new(io::ErrorKind::NotConnected, "Disconnected")));
        }

        Box::new(rx.map_err(|_| io::Error::new(io::ErrorKind::ConnectionAborted, "Restart"))
                   .and_then(|res| res)
                   .actfuture())
    }
}
