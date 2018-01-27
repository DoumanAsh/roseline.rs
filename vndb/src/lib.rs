extern crate vndb;
extern crate tokio_core;
extern crate futures;

#[macro_use(slog_debug, slog_info, slog_warn, slog_error, slog_log, slog_record, slog_b, slog_kv, slog_record_static)]
extern crate slog;
#[macro_use]
extern crate slog_scope;

use futures::future::Future;

use std::io;
use std::cell::Cell;
use std::rc::Rc;

pub use self::vndb::protocol::message;
use self::vndb::client::tokio::{
    RcSender,
    RcReceiver,
    PendingConnect,
    PendingRequest,
    FutureResponse,
    RcClient
};

pub type RequestType = message::request::get::Type;

enum RequestState {
    None,
    Sent(PendingRequest<RcSender>),
    Wait(FutureResponse<RcReceiver>)
}

impl Default for RequestState {
    fn default() -> Self {
        RequestState::None
    }
}

///Ongoing VNDB request that self recovers from connection loss.
pub struct VndbRequest {
    client: Client,
    request: message::Request,
    state: Cell<RequestState>
}

impl Future for VndbRequest {
    type Item = message::Response;
    type Error = io::Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        match self.client.poll()? {
            futures::Async::Ready(_) => (),
            futures::Async::NotReady => return Ok(futures::Async::NotReady)
        }

        let state = unsafe { &*(self.client.state.as_ptr()) };
        let client = match state {
            &State::Connected(ref client) => client,
            _ => {
                warn!("Client poll returned ready, but state is still not connected!");
                return Ok(futures::Async::NotReady);
            }
        };

        loop {
            let state = self.state.take();
            match state {
                RequestState::None => {
                    let send = client.send(self.request.clone());
                    self.state.set(RequestState::Sent(send));
                    //Loop again to drive IO.
                },
                RequestState::Sent(mut pending) => {
                    match pending.poll() {
                        Ok(futures::Async::Ready(_)) => {
                            debug!("VNDB: request is sent");
                            self.state.set(RequestState::Wait(client.receive()));
                            //Loop again to drive IO.
                        },
                        Ok(futures::Async::NotReady) => {
                            self.state.set(RequestState::Sent(pending));
                            return Ok(futures::Async::NotReady);
                        },
                        Err(error) => {
                            warn!("VNDB: request failed with error: {}", error);
                            self.state.set(RequestState::None);
                            self.client.state.set(State::None);
                            //Restart client by driving IO.
                            return self.poll();
                        }
                    }
                },
                RequestState::Wait(mut pending) => {
                    match pending.poll() {
                        Ok(futures::Async::Ready(result)) => match result {
                            (Some(rsp), _) => {
                                debug!("VNDB: response has been received");
                                return Ok(futures::Async::Ready(rsp))
                            },
                            (None, _) => {
                                warn!("VNDB: connection is unexpectedly closed. Restart request...");
                                self.state.set(RequestState::None);
                                self.client.state.set(State::None);
                                //Restart client by driving IO.
                                return self.poll();
                            }
                        },
                        Ok(futures::Async::NotReady) => {
                            self.state.set(RequestState::Wait(pending));
                            return Ok(futures::Async::NotReady);
                        },
                        Err(error) => {
                            warn!("VNDB: Failed to get response. Error: {}", error);
                            self.state.set(RequestState::None);
                            self.client.state.set(State::None);
                            //On connection error we need to restart client.
                            return self.poll();
                        }
                    }
                }
            }
        }
    }
}

///State of client.
enum State {
    None,
    Connecting(PendingConnect<RcSender, RcReceiver>),
    Login(RcClient, PendingRequest<RcSender>),
    WaitLogin(RcClient, FutureResponse<RcReceiver>),
    Connected(RcClient)
}

impl Default for State {
    fn default() -> Self {
        State::None
    }
}

#[derive(Clone)]
///Wrapper over VNDB client.
pub struct Client {
    handle: tokio_core::reactor::Handle,
    state: Rc<Cell<State>>
}

impl Client {
    pub fn new(handle: tokio_core::reactor::Handle) -> io::Result<Self> {
        Ok(Client {
            handle: handle.clone(),
            state: Rc::new(Cell::new(State::Connecting(RcClient::new(&handle)?)))
        })
    }

    fn poll(&self) -> futures::Poll<(), io::Error> {
        loop {
            let state = self.state.take();
            match state {
                State::None => {
                    info!("VNDB: Starting...");
                    let new_connection = match RcClient::new(&self.handle) {
                        Ok(new_connection) => new_connection,
                        Err(error) => {
                            error!("Unexpected IO error on re-connecting to VNDB: {}", error);
                            return Err(error);
                        }
                    };
                    info!("VNDB: Conneting...");
                    self.state.set(State::Connecting(new_connection));
                    return Ok(futures::Async::NotReady)
                },
                State::Connecting(mut pending) => match pending.poll() {
                    Ok(result) => match result {
                        futures::Async::Ready(new_client) => {
                            info!("VNDB: Connected.");
                            let login = new_client.send(message::request::Login::new(None, None));
                            self.state.set(State::Login(new_client, login));
                            //Loop again to drive IO
                        },
                        futures::Async::NotReady => {
                            debug!("VNDB: Waiting for connection");
                            self.state.set(State::Connecting(pending));
                            return Ok(futures::Async::NotReady)
                        }
                    },
                    Err(error) => {
                        error!("VNDB: Connection failure. Error: {}", error);
                        self.state.set(State::None);
                        //Loop again to drive IO
                    }
                },
                State::Login(client, mut pending_login) => match pending_login.poll() {
                    Ok(result) => match result {
                        futures::Async::Ready(_) => {
                            info!("VNDB: Sent login server");
                            let login_rsp = client.receive();
                            self.state.set(State::WaitLogin(client, login_rsp));
                            //Loop again to drive IO
                        },
                        _ => {
                            debug!("VNDB: Waiting to send request");
                            self.state.set(State::Login(client, pending_login));
                            return Ok(futures::Async::NotReady)
                        }
                    },
                    Err(error) => {
                        error!("VNDB: Connection failure. Error: {}", error);
                        self.state.set(State::None);
                        //Loop again to drive IO
                    }
                },
                State::WaitLogin(client, mut login_response) => match login_response.poll() {
                    Ok(result) => match result {
                        futures::Async::Ready((Some(message::Response::Ok), _)) => {
                            info!("VNDB: Successfully connected");
                            self.state.set(State::Connected(client));
                            return Ok(futures::Async::Ready(()))
                        },
                        futures::Async::Ready((Some(rsp), _)) => {
                            error!("VNDB: Failed to login. Error: {:?}", rsp);
                            let login = client.send(message::request::Login::new(None, None));
                            self.state.set(State::Login(client, login));
                            //Loop again to drive IO
                        },
                        futures::Async::Ready((None, _)) => {
                            warn!("VNDB: Connection is unexpectedly closed. Restart...");
                            self.state.set(State::None);
                            //Loop again to drive IO
                        }
                        _ => {
                            debug!("VNDB: Waiting for auth");
                            self.state.set(State::WaitLogin(client, login_response));
                            return Ok(futures::Async::NotReady)
                        }
                    },
                    Err(error) => {
                        error!("VNDB: Connection failure. Error: {}", error);
                        self.state.set(State::None);
                        //Loop again to drive IO
                    }
                },
                State::Connected(client) => {
                    self.state.set(State::Connected(client));
                    return Ok(futures::Async::Ready(()))
                }
            }
        }
    }

    pub fn get_by_id(&self, kind: message::request::get::Type, id: u64) -> VndbRequest {
        let get: message::Request = message::request::Get {
            kind,
            flags: message::request::get::Flags::new().basic(),
            filters: message::request::get::Filters::new().filter(format_args!("id = {}", id)),
            options: None
        }.into();

        VndbRequest {
            client: self.clone(),
            request: get.clone(),
            state: Cell::new(RequestState::None)
        }
    }

    #[inline]
    pub fn get_vn_by_id(&self, id: u64) -> VndbRequest {
        self.get_by_id(message::request::get::Type::vn(), id)
    }

    pub fn get_vn_by_filter(&self, filters: message::request::get::Filters) -> VndbRequest {
        let get: message::Request = message::request::Get {
            kind: message::request::get::Type::vn(),
            flags: message::request::get::Flags::new().basic(),
            filters,
            options: None
        }.into();

        VndbRequest {
            client: self.clone(),
            request: get.clone(),
            state: Cell::new(RequestState::None)
        }
    }

    #[inline]
    ///Looks VN with exact title
    pub fn get_vn_by_title(&self, title: &str) -> VndbRequest {
        debug!("VNDB: get vn by title='{}'", title);
        let filters = message::request::get::Filters::new().filter(format_args!("title = \"{}\"", title))
                                                           .or(format_args!("original = \"{}\"", title));
        self.get_vn_by_filter(filters)
    }

    #[inline]
    ///Search VN with partial title.
    pub fn look_up_vn_by_title(&self, title: &str) -> VndbRequest {
        debug!("VNDB: look vn by title='{}'", title);
        let filters = message::request::get::Filters::new().filter(format_args!("title ~ \"{}\"", title))
                                                           .or(format_args!("original ~ \"{}\"", title));
        self.get_vn_by_filter(filters)
    }
}

#[cfg(test)]
mod tests {
    use super::message;
    use super::tokio_core;
    use super::Client;

    #[test]
    fn get_vn_id() {
        let mut tokio_core = tokio_core::reactor::Core::new().expect("Should create tokio core");
        let client = Client::new(tokio_core.handle()).expect("Should create client initially");

        let result = tokio_core.run(client.get_by_id(message::request::get::Type::vn(), 1)).expect("Should get VN by id");
        match result {
            message::Response::Results(vn) => {
                let result = vn.vn().expect("Should convert to VN");
                assert_eq!(result.num, 1);
                let vn = &result.items[0];
                assert_eq!(vn.id, 1);
            },
            result => assert!(false, format!("Unexpected response {:?}", result))
        }

        let result = tokio_core.run(client.get_by_id(message::request::get::Type::vn(), 2)).expect("Should get VN by id");
        match result {
            message::Response::Results(vn) => {
                let result = vn.vn().expect("Should convert to VN");
                assert_eq!(result.num, 1);
                let vn = &result.items[0];
                assert_eq!(vn.id, 2);
            },
            result => assert!(false, format!("Unexpected response {:?}", result))
        }
    }
}
