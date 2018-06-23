extern crate futures;
extern crate actix;
extern crate vndb;

use self::futures::Future;
use self::futures::unsync::oneshot;
use self::actix::prelude::*;
pub use self::vndb::{protocol, client};

use ::collections::VecDeque;
use ::time;
use ::io;

pub struct Vndb {
    sender: Option<client::tokio::ClientSender>,
    queue: VecDeque<oneshot::Sender<io::Result<protocol::message::Response>>>,
    //Controls restart delay
    //In case of constant failures it
    //increases with each restart.
    timeout: u64
}

impl Vndb {
    pub fn new() -> Self {
        Self {
            sender: None,
            queue: VecDeque::with_capacity(10),
            timeout: 0
        }
    }

    #[inline]
    fn reset_timeout(&mut self) {
        self.timeout = 0;
    }

    //Schedules to restart self
    fn restart_later(&mut self, ctx: &mut Context<Self>) {
        const TIMEOUT_INTERVAL: u64 = 1_000;
        const TIMEOUT_MAX: u64 = 5_000;
        if self.timeout < TIMEOUT_MAX {
            self.timeout += TIMEOUT_INTERVAL;
        }

        ctx.run_later(time::Duration::new(self.timeout, 0), |_, ctx| ctx.stop());
    }
}

impl Actor for Vndb {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        let vndb = match client::tokio::Client::new() {
            Ok(vndb) => vndb,
            Err(error) => {
                error!("VNDB: Failed to create Client. Error: {}", error);
                self.restart_later(ctx);
                return
            }
        };

        let vndb = vndb.into_actor(self).map_err(|error, act, ctx| {
            error!("VNDB: Unable to connect. Error: {}", error);
            act.restart_later(ctx);
        }).map(|client, act, ctx| {
            info!("VNDB: Connected.");
            let (sink, stream) = client.into_parts();

            match sink.request(protocol::message::request::Login::new(None, None)) {
                Ok(_) => info!("VNDB: Sent Login message"),
                Err(error) => {
                    warn!("VNDB: Unable to send login message. Error: {}", error);
                    return act.restart_later(ctx);
                }
            }
            Self::add_stream(stream, ctx);

            act.sender = Some(sink);
            act.reset_timeout();
        });

        ctx.spawn(vndb);
    }
}

impl Supervised for Vndb {
    fn restarting(&mut self, _: &mut Self::Context) {
        info!("VNDB: Restarting...");
        self.sender.take();
        for tx in self.queue.drain(..) {
            let _ = tx.send(Err(io::Error::new(io::ErrorKind::ConnectionAborted, "Restart")));
        }
    }
}

impl StreamHandler2<protocol::message::Response, io::Error> for Vndb {
    fn handle(&mut self, msg: Result<Option<protocol::message::Response>, io::Error>, ctx: &mut Self::Context) {
        let msg = match msg {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                warn!("VNDB: Connection is closed");
                return ctx.stop();
            },
            Err(error) => {
                warn!("VNDB: IO error: {}", error);
                return ctx.stop();

            }
        };

        trace!("VNDB: receives {:?}", msg);
        match self.queue.pop_front() {
            Some(tx) => {
                let _ = tx.send(Ok(msg));
            },
            None => {
                match msg {
                    //As we only use Get methods, OK can be received on login only.
                    protocol::message::Response::Ok => info!("VNDB: Login complete"),
                    msg => warn!("Received message while there was no request. Message={:?}", msg)
                }
            }
        }
    }
}

pub use self::protocol::message::request::get::{Type, Flags, Filters, Options};
pub use self::protocol::message::{Response, response};

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

    pub fn get_by_id(kind: Type, id: u64) -> Self {
        let filters = Filters::new().filter(format_args!("id = {}", id));

        Self::new(kind, Flags::new().basic(), filters, None)
    }

    #[inline]
    pub fn vn_by_id(id: u64) -> Self {
        Self::get_by_id(Type::vn(), id)
    }

    pub fn vn_by_exact_title(title: &str) -> Self {
        let filters = Filters::new().filter(format_args!("title = \"{}\"", title))
                                    .or(format_args!("original = \"{}\"", title));

        Self::new(Type::vn(), Flags::new().basic(), filters, None)
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

impl Message for Request {
    type Result = io::Result<protocol::message::Response>;
}

impl Handler<Request> for Vndb {
    type Result = ResponseFuture<protocol::message::Response, io::Error>;

    fn handle(&mut self, msg: Request, ctx: &mut Self::Context) -> Self::Result {
        trace!("VNDB: send {}", &msg.0);

        let (tx, rx) = oneshot::channel();
        let send = self.sender.as_mut().and_then(|sender| sender.request(msg.0).ok());
        match send {
            Some(_) => self.queue.push_back(tx),
            None => {
                //In unlikely case sender invalidation
                //we should restart Actor, but we still need to response
                let _ = tx.send(Err(io::Error::new(io::ErrorKind::NotConnected, "Disconnected")));
                ctx.stop();
            }
        }

        Box::new(rx.map_err(|_| io::Error::new(io::ErrorKind::ConnectionAborted, "Restart"))
                   .and_then(|res| res))
    }
}
