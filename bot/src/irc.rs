extern crate irc;
extern crate actix;
extern crate futures;
extern crate utils;
extern crate actors;
extern crate regex;

use self::futures::{
    Stream
};
use self::actix::prelude::*;
use self::irc::client::{
    Client,
    IrcClient,
    PackedIrcClient,
    IrcClientFuture
};
use self::irc::proto::command::Command;
use self::irc::proto::message::Message as InnerIrcMessage;
use self::irc::error::IrcError;
use self::irc::client::ext::ClientExt;
use self::utils::duration;

use ::config::Config;
use ::command;

macro_rules! try_option {
    ($result:expr, $warn:expr) => { match $result {
        Some(result) => result,
        None => {
            warn!($warn);
            return;
        }
    }}
}

const CMD_DELAY_MS: u64 = 500;

pub struct Irc {
    config: Config,
    vndb: Addr<Unsync, actors::vndb::Vndb>,
    db: Addr<Syn, actors::db::Db>,
    client: Option<IrcClient>
}

impl Irc {
    pub fn new(config: Config, vndb: Addr<Unsync, actors::vndb::Vndb>, db: Addr<Syn, actors::db::Db>) -> Self {
        Self {
            config,
            vndb,
            db,
            client: None
        }
    }
}

#[derive(Debug)]
struct IrcMessage(pub InnerIrcMessage);

impl Message for IrcMessage {
    type Result = Result<(), IrcError>;
}

impl StreamHandler<IrcMessage, IrcError> for Irc {
    fn finished(&mut self, ctx: &mut Self::Context) {
        warn!("IRC: Connection is closed");
        ctx.stop();
    }

    fn error(&mut self, error: IrcError, ctx: &mut Self::Context) -> Running {
        warn!("IRC: Reading IO error: {}", error);
        ctx.stop();
        Running::Stop
    }

    fn handle(&mut self, msg: IrcMessage, ctx: &mut Self::Context) {
        trace!("IRC: message={:?}", msg);

        let msg = msg.0;
        let from = msg.prefix.as_ref().map(|prefix| &prefix[..prefix.find('!').unwrap_or(0)]);
        let client = self.client.as_ref().unwrap();

        match msg.command {
            Command::PRIVMSG(target, msg) => {
                info!("from {:?} to {}: {}", from, target, msg);

                let cmd = match command::Command::from_str(&msg) {
                    Some(cmd) => cmd,
                    None => return
                };

                let is_pm = client.current_nickname() == target;
                let from = from.unwrap().to_string();

                match cmd {
                    command::Command::Text(text) => ctx.notify(TextResponse::new(target, from, is_pm, text)),
                    command::Command::GetVn(get_vn) => ctx.notify(GetVnResponse::new(target, from, is_pm, get_vn)),
                    command::Command::GetHookByTitle(get_hook) => ctx.notify(GetHookByTitleResponse::new(target, from, is_pm, get_hook)),
                    command::Command::GetHookById(get_hook) => ctx.notify(GetHookByIdResponse::new(target, from, is_pm, get_hook)),
                    command::Command::SetHookByTitle(set_hook) => ctx.notify(SetHookByTitleResponse::new(target, from, is_pm, set_hook)),
                    command::Command::SetHookById(set_hook) => ctx.notify(SetHookByIdResponse::new(target, from, is_pm, set_hook)),
                    command::Command::DelHookByTitle(del_hook) => ctx.notify(DelHookByTitleResponse::new(target, from, is_pm, del_hook)),
                    command::Command::DelHookById(del_hook) => ctx.notify(DelHookByIdResponse::new(target, from, is_pm, del_hook)),
                    command::Command::DelVnByTitle(del_vn) => ctx.notify(DelVnByTitleResponse::new(target, from, is_pm, del_vn)),
                    command::Command::DelVnById(del_vn) => ctx.notify(DelVnByIdResponse::new(target, from, is_pm, del_vn)),
                    command::Command::Refs(mut refs) => for reference in &mut refs.refs {
                        let reference = reference.take();
                        if let Some(reference) = reference {
                            ctx.notify(GetRefResponse::new(target.clone(), from.clone(), is_pm, reference));
                        }
                        else {
                            break;
                        }
                    }
                }
            },
            Command::JOIN(chanlist, _, _) => info!("{:?} joined {}", from, chanlist),
            Command::PART(chanlist, _) => info!("{:?} left {}", from, chanlist),
            Command::KICK(chanlist, user, _) => {
                info!("{:?} kicked {} out of {}", from, user, chanlist);
                if user == client.current_nickname() {
                    ctx.run_later(duration::ms(500), move |act, ctx| {
                        match act.client.as_ref().unwrap().send_join(&chanlist) {
                            Ok(_) => (),
                            Err(error) => {
                                error!("IRC: Error occured: {}", error);
                                ctx.stop();
                            }
                        }
                    });
                }
            },
            msg => debug!("Unhandled message={:?}", msg)
        }
    }
}

pub struct GetIrcResponse<T> {
    target: String,
    from: String,
    is_pm: bool,
    cmd: T
}

impl<T> GetIrcResponse<T> {
    pub fn new(target: String, from: String, is_pm: bool, cmd: T) -> Self {
        GetIrcResponse {
            target,
            from,
            is_pm,
            cmd
        }
    }

    pub fn respond(&self, client: &IrcClient, msg: &str) -> Result<(), IrcError> {
        match self.is_pm {
            true => client.send_privmsg(&self.from, msg),
            false => client.send_privmsg(&self.target, &format!("{}: {}", self.from, msg))
        }
    }
}

impl<T> Message for GetIrcResponse<T> {
    type Result = Result<(), IrcError>;
}

type TextResponse = GetIrcResponse<command::Text>;
impl Handler<TextResponse> for Irc {
    type Result = <TextResponse as Message>::Result;

    fn handle(&mut self, msg: TextResponse, _: &mut Self::Context) -> Self::Result {
        let client = self.client.as_ref().unwrap();

        msg.respond(client, &msg.cmd.0)
    }
}

//.vn
type GetVnResponse = GetIrcResponse<command::GetVn>;
impl Handler<GetVnResponse> for Irc {
    type Result = <GetVnResponse as Message>::Result;

    fn handle(&mut self, msg: GetVnResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetVnResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let get_vn = actors::vndb::Get::vn_by_exact_title(&title);
        let get_vn = self.vndb.send(get_vn.into()).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let text = format!("{} - https://vndb.org/v{}", vn.title.as_ref().unwrap(), vn.id);
                            ctx.notify(TextResponse::new(target, from, is_pm, text.into()))
                        },
                        _ => ctx.notify(SearchVnResponse::new(target, from, is_pm, command::SearchVn { title }))
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("GetVnResponse Error: '{}'. Re-try", error);
                ctx.notify_later(GetVnResponse::new(target, from, is_pm, command::GetVn{ title }), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetVn: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type SearchVnResponse = GetIrcResponse<command::SearchVn>;
impl Handler<SearchVnResponse> for Irc {
    type Result = <SearchVnResponse as Message>::Result;

    fn handle(&mut self, msg: SearchVnResponse, ctx: &mut Self::Context) -> Self::Result {
        let SearchVnResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let search_vn = actors::vndb::Get::vn_by_title(&title);
        let search_vn = self.vndb.send(search_vn.into()).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        0 => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::no_such_vn())),
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let text = format!("{} - https://vndb.org/v{}", vn.title.as_ref().unwrap(), vn.id);
                            ctx.notify(TextResponse::new(target, from, is_pm, text.into()))
                        },
                        num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::too_many_vn_hits(num, title)))
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("SearchVnResponse Error: '{}'. Re-try", error);
                ctx.notify_later(SearchVnResponse::new(target, from, is_pm, command::SearchVn{ title }), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing SearchVn: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

//.set_hook
type SetHookByExactVndbTitleResponse = GetIrcResponse<command::SetHookByExactVndbTitle>;
impl Handler<SetHookByExactVndbTitleResponse> for Irc {
    type Result = <SetHookByExactVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: SetHookByExactVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetHookByExactVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let command::SetHookByExactVndbTitle {title, version, code} = cmd;

        let get_vn = actors::vndb::Get::vn_by_exact_title(&title);
        let get_vn = self.vndb.send(get_vn.into()).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(mut vn) => match vn.items.len() {
                        1 => {
                            let vn = vn.items.pop().unwrap();
                            let id = vn.id;
                            let title = vn.title.unwrap();
                            let set_new_hook = command::SetNewHook { id, title, version, code};
                            ctx.notify(SetNewHookResponse::new(target, from, is_pm, set_new_hook))
                        },
                        _ => ctx.notify(SetHookByVndbTitleResponse::new(target, from, is_pm, command::SetHookByVndbTitle { title, version, code })),
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("SetHookByExactVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::SetHookByExactVndbTitle {title, version, code};
                ctx.notify_later(SetHookByExactVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing SetHook: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type SetHookByVndbTitleResponse = GetIrcResponse<command::SetHookByVndbTitle>;
impl Handler<SetHookByVndbTitleResponse> for Irc {
    type Result = <SetHookByVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: SetHookByVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetHookByVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let command::SetHookByVndbTitle {title, version, code} = cmd;

        let search_vn = actors::vndb::Get::vn_by_title(&title);
        let search_vn = self.vndb.send(search_vn.into()).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(mut vn) => match vn.items.len() {
                        0 => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::no_such_vn())),
                        1 => {
                            let vn = vn.items.pop().unwrap();
                            let id = vn.id;
                            let title = vn.title.unwrap();
                            let set_new_hook = command::SetNewHook { id, title, version, code};
                            ctx.notify(SetNewHookResponse::new(target, from, is_pm, set_new_hook))
                        },
                        num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::too_many_vn_hits(num, title)))
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("SetHookByVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::SetHookByVndbTitle {title, version, code};
                ctx.notify_later(SetHookByVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing SetHook: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type SetHookByTitleResponse = GetIrcResponse<command::SetHookByTitle>;
impl Handler<SetHookByTitleResponse> for Irc {
    type Result = <SetHookByTitleResponse as Message>::Result;

    fn handle(&mut self, msg: SetHookByTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetHookByTitleResponse {target, from, is_pm, cmd} = msg;
        let command::SetHookByTitle {title, version, code} = cmd;

        let search_vn = actors::db::SearchVn(title.clone());
        let search_vn = self.db.send(search_vn).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(mut vns) => match vns.len() {
                0 => ctx.notify(SetHookByExactVndbTitleResponse::new(target, from, is_pm, command::SetHookByExactVndbTitle { title, version, code })),
                1 => {
                    let vn = vns.pop().unwrap();
                    let set_vn = command::SetVnHook { vn, version, code};
                    ctx.notify(SetHookForVnResponse::new(target, from, is_pm, set_vn))
                },
                num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::better_vn_query(num, &title))),
            },
            Err(error) => {
                warn!("SetHookByTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::SetHookByTitle {title, version, code};
                ctx.notify_later(SetHookByTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing SetHook: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type SetHookForVnResponse = GetIrcResponse<command::SetVnHook>;
impl Handler<SetHookForVnResponse> for Irc {
    type Result = <SetHookForVnResponse as Message>::Result;

    fn handle(&mut self, msg: SetHookForVnResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetHookForVnResponse {target, from, is_pm, cmd} = msg;
        let command::SetVnHook {vn, version, code} = cmd;
        let title = vn.title.clone();

        let put_hook = actors::db::PutHook { vn, version, code };
        let put_hook = self.db.send(put_hook).into_actor(self);
        let put_hook = put_hook.map(move |result, _act, ctx| match result {
            Ok(result) => ctx.notify(TextResponse::new(target, from, is_pm, format!("Added hook '{}' for VN: {}", result.code, title).into())),
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error))),
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing PutHook: {}", error)
        });
        ctx.spawn(put_hook);

        Ok(())
    }
}

type SetNewHookResponse = GetIrcResponse<command::SetNewHook>;
impl Handler<SetNewHookResponse> for Irc {
    type Result = <SetNewHookResponse as Message>::Result;

    fn handle(&mut self, msg: SetNewHookResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetNewHookResponse {target, from, is_pm, cmd} = msg;
        let command::SetNewHook {id, title, version, code} = cmd;

        let put_vn = actors::db::PutVn { id, title };
        let put_vn = self.db.send(put_vn).into_actor(self);
        let put_vn = put_vn.map(move |result, _act, ctx| match result {
            Ok(result) => ctx.notify(SetHookForVnResponse::new(target, from, is_pm, command::SetVnHook { vn: result, version, code })),
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error))),
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing PutVn: {}", error)
        });
        ctx.spawn(put_vn);

        Ok(())
    }
}

type SetHookByNewIdResponse = GetIrcResponse<command::SetHookByNewId>;
impl Handler<SetHookByNewIdResponse> for Irc {
    type Result = <SetHookByNewIdResponse as Message>::Result;

    fn handle(&mut self, msg: SetHookByNewIdResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetHookByNewIdResponse {target, from, is_pm, cmd} = msg;
        let command::SetHookByNewId {id, version, code} = cmd;

        let get_vn = actors::vndb::Get::vn_by_id(id);
        let get_vn = self.vndb.send(get_vn.into()).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(mut vn) => match vn.items.len() {
                        1 => {
                            let vn = vn.items.pop().unwrap();
                            let title = vn.title.unwrap();
                            ctx.notify(SetNewHookResponse::new(target, from, is_pm, command::SetNewHook { id, title, version, code }))
                        },
                        num => {
                            error!("Unexpected number of VNDB items in request by id '{}'", num);
                            ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                        }
                    },
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("SetHookByNewIdResponse Error: '{}'. Re-try", error);
                let cmd = command::SetHookByNewId {id, version, code};
                ctx.notify_later(SetHookByNewIdResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing SetHookByNewId: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type SetHookByIdResponse = GetIrcResponse<command::SetHookById>;
impl Handler<SetHookByIdResponse> for Irc {
    type Result = <SetHookByIdResponse as Message>::Result;

    fn handle(&mut self, msg: SetHookByIdResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetHookByIdResponse {target, from, is_pm, cmd} = msg;
        let command::SetHookById {id, version, code} = cmd;

        let get_vn = actors::db::GetVnData(id);
        let get_vn = self.db.send(get_vn).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(Some(result)) => ctx.notify(SetHookForVnResponse::new(target, from, is_pm, command::SetVnHook { vn: result.data, version, code})),
            Ok(None) => ctx.notify(SetHookByNewIdResponse::new(target, from, is_pm, command::SetHookByNewId { id, version, code })),
            Err(error) => {
                error!("DB error: {}", error);
                ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
            },
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing SetHookById: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

//.hook
type GetHookByExactVndbTitleResponse = GetIrcResponse<command::GetHookByExactVndbTitle>;
impl Handler<GetHookByExactVndbTitleResponse> for Irc {
    type Result = <GetHookByExactVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: GetHookByExactVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetHookByExactVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let get_vn = actors::vndb::Get::vn_by_exact_title(&title);
        let get_vn = self.vndb.send(get_vn.into()).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let id = vn.id;
                            ctx.notify(GetHookByIdResponse::new(target, from, is_pm, command::GetHookById { id }))
                        },
                        _ => ctx.notify(GetHookByVndbTitleResponse::new(target, from, is_pm, command::GetHookByVndbTitle { title })),
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("GetHookByExactVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::GetHookByExactVndbTitle {title};
                ctx.notify_later(GetHookByExactVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetHook: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type GetHookByVndbTitleResponse = GetIrcResponse<command::GetHookByVndbTitle>;
impl Handler<GetHookByVndbTitleResponse> for Irc {
    type Result = <GetHookByVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: GetHookByVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetHookByVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let search_vn = actors::vndb::Get::vn_by_title(&title);
        let search_vn = self.vndb.send(search_vn.into()).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        0 => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::no_such_vn())),
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let id = vn.id;
                            ctx.notify(GetHookByIdResponse::new(target, from, is_pm, command::GetHookById { id }))
                        },
                        num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::too_many_vn_hits(num, title)))
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("GetHookByVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::GetHookByVndbTitle {title};
                ctx.notify_later(GetHookByVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetHook: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type GetHookByTitleResponse = GetIrcResponse<command::GetHookByTitle>;
impl Handler<GetHookByTitleResponse> for Irc {
    type Result = <GetHookByTitleResponse as Message>::Result;

    fn handle(&mut self, msg: GetHookByTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetHookByTitleResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let search_vn = actors::db::SearchVn(title.clone());
        let search_vn = self.db.send(search_vn).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(vns) => match vns.len() {
                0 => ctx.notify(GetHookByExactVndbTitleResponse::new(target, from, is_pm, command::GetHookByExactVndbTitle { title })),
                1 => {
                    let vn = unsafe { vns.get_unchecked(0) };
                    ctx.notify(GetHookByIdResponse::new(target, from, is_pm, command::GetHookById { id: vn.id as u64 }))
                },
                num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::better_vn_query(num, &title))),
            },
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error))),
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetHook: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type GetHookForVnResponse = GetIrcResponse<actors::db::models::Vn>;
impl Handler<GetHookForVnResponse> for Irc {
    type Result = <GetHookForVnResponse as Message>::Result;

    fn handle(&mut self, msg: GetHookForVnResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetHookForVnResponse {target, from, is_pm, cmd} = msg;

        let get_vn = actors::db::GetHooks(cmd);
        let get_vn = self.db.send(get_vn).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(result) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::from_vn_data(result))),
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error))),
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetHookById: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type GetHookByIdResponse = GetIrcResponse<command::GetHookById>;
impl Handler<GetHookByIdResponse> for Irc {
    type Result = <GetHookByIdResponse as Message>::Result;

    fn handle(&mut self, msg: GetHookByIdResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetHookByIdResponse {target, from, is_pm, cmd} = msg;
        let id = cmd.id;

        let get_vn = actors::db::GetVnData(id);
        let get_vn = self.db.send(get_vn).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(Some(result)) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::from_vn_data(result))),
            Ok(None) => ctx.notify(TextResponse::new(target, from, is_pm, "No hook exists for VN".into())),
            Err(error) => {
                error!("DB error: {}", error);
                ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
            },
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetHookById: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

//.del_hook
type DelHookByExactVndbTitleResponse = GetIrcResponse<command::DelHookByExactVndbTitle>;
impl Handler<DelHookByExactVndbTitleResponse> for Irc {
    type Result = <DelHookByExactVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: DelHookByExactVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelHookByExactVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let command::DelHookByExactVndbTitle {title, version} = cmd;

        let get_vn = actors::vndb::Get::vn_by_exact_title(&title);
        let get_vn = self.vndb.send(get_vn.into()).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let id = vn.id;
                            ctx.notify(DelHookByIdResponse::new(target, from, is_pm, command::DelHookById { id, version }))
                        },
                        _ => ctx.notify(DelHookByVndbTitleResponse::new(target, from, is_pm, command::DelHookByVndbTitle { title, version })),
                    },
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("DelHookByExactVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::DelHookByExactVndbTitle {title, version};
                ctx.notify_later(DelHookByExactVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelHook: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type DelHookByVndbTitleResponse = GetIrcResponse<command::DelHookByVndbTitle>;
impl Handler<DelHookByVndbTitleResponse> for Irc {
    type Result = <DelHookByVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: DelHookByVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelHookByVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let command::DelHookByVndbTitle {title, version} = cmd;

        let search_vn = actors::vndb::Get::vn_by_title(&title);
        let search_vn = self.vndb.send(search_vn.into()).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        0 => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::no_such_vn())),
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let id = vn.id;
                            ctx.notify(DelHookByIdResponse::new(target, from, is_pm, command::DelHookById { id, version }))
                        },
                        num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::too_many_vn_hits(num, title)))
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("DelHookByVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::DelHookByVndbTitle {title, version};
                ctx.notify_later(DelHookByVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelHook: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type DelHookByTitleResponse = GetIrcResponse<command::DelHookByTitle>;
impl Handler<DelHookByTitleResponse> for Irc {
    type Result = <DelHookByTitleResponse as Message>::Result;

    fn handle(&mut self, msg: DelHookByTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelHookByTitleResponse {target, from, is_pm, cmd} = msg;
        let command::DelHookByTitle {title, version} = cmd;

        let search_vn = actors::db::SearchVn(title.clone());
        let search_vn = self.db.send(search_vn).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(vns) => match vns.len() {
                0 => ctx.notify(DelHookByExactVndbTitleResponse::new(target, from, is_pm, command::DelHookByExactVndbTitle { title, version })),
                1 => {
                    let vn = unsafe { vns.get_unchecked(0) };
                    ctx.notify(DelHookByIdResponse::new(target, from, is_pm, command::DelHookById { id: vn.id as u64, version }))
                },
                num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::better_vn_query(num, &title))),
            },
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error))),
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelHook: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type DelHookByIdResponse = GetIrcResponse<command::DelHookById>;
impl Handler<DelHookByIdResponse> for Irc {
    type Result = <DelHookByIdResponse as Message>::Result;

    fn handle(&mut self, msg: DelHookByIdResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelHookByIdResponse {target, from, is_pm, cmd} = msg;
        let command::DelHookById {id, version} = cmd;

        let get_vn = actors::db::GetVn(id);
        let get_vn = self.db.send(get_vn).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(Some(vn)) => ctx.notify(DelVnHookResponse::new(target, from, is_pm, command::DelVnHook { vn, version })),
            Ok(None) => ctx.notify(TextResponse::new(target, from, is_pm, format!("No hook already for v{}", id).into())),
            Err(error) => {
                error!("DB error: {}", error);
                ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
            },
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelHookById: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type DelVnHookResponse = GetIrcResponse<command::DelVnHook>;
impl Handler<DelVnHookResponse> for Irc {
    type Result = <DelVnHookResponse as Message>::Result;

    fn handle(&mut self, msg: DelVnHookResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelVnHookResponse {target, from, is_pm, cmd} = msg;
        let command::DelVnHook {vn, version} = cmd;
        let id = vn.id;

        info!("Attempt to remove hook '{}' for VN {}", &version, &vn.title);
        let del_hook = actors::db::DelHook { vn, version };
        let del_hook = self.db.send(del_hook).into_actor(self);
        let del_hook = del_hook.map(move |result, _act, ctx| match result {
            Ok(0) => ctx.notify(TextResponse::new(target, from, is_pm, format!("v{}: No such hook to remove", id).into())),
            Ok(_) => ctx.notify(TextResponse::new(target, from, is_pm, format!("v{}: Removed hook", id).into())),
            Err(error) => {
                error!("DB error: {}", error);
                ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelVnHook: {}", error)
        });
        ctx.spawn(del_hook);

        Ok(())
    }
}

//.del_vn
type DelVnByExactVndbTitleResponse = GetIrcResponse<command::DelVnByExactVndbTitle>;
impl Handler<DelVnByExactVndbTitleResponse> for Irc {
    type Result = <DelVnByExactVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: DelVnByExactVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelVnByExactVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let get_vn = actors::vndb::Get::vn_by_exact_title(&title);
        let get_vn = self.vndb.send(get_vn.into()).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let id = vn.id;
                            ctx.notify(DelVnByIdResponse::new(target, from, is_pm, command::DelVnById { id }))
                        },
                        _ => ctx.notify(DelVnByVndbTitleResponse::new(target, from, is_pm, command::DelVnByVndbTitle { title })),
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("DelVnByExactVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::DelVnByExactVndbTitle {title};
                ctx.notify_later(DelVnByExactVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelVn: {}", error)
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

type DelVnByVndbTitleResponse = GetIrcResponse<command::DelVnByVndbTitle>;
impl Handler<DelVnByVndbTitleResponse> for Irc {
    type Result = <DelVnByVndbTitleResponse as Message>::Result;

    fn handle(&mut self, msg: DelVnByVndbTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelVnByVndbTitleResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let search_vn = actors::vndb::Get::vn_by_title(&title);
        let search_vn = self.vndb.send(search_vn.into()).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => match result.vn() {
                    Ok(vn) => match vn.items.len() {
                        0 => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::no_such_vn())),
                        1 => {
                            let vn = unsafe { vn.items.get_unchecked(0) };
                            let id = vn.id;
                            ctx.notify(DelVnByIdResponse::new(target, from, is_pm, command::DelVnById { id }))
                        },
                        num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::too_many_vn_hits(num, title)))
                    }
                    Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("DelVnByVndbTitleResponse Error: '{}'. Re-try", error);
                let cmd = command::DelVnByVndbTitle {title};
                ctx.notify_later(DelVnByVndbTitleResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelVn: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type DelVnByTitleResponse = GetIrcResponse<command::DelVnByTitle>;
impl Handler<DelVnByTitleResponse> for Irc {
    type Result = <DelVnByTitleResponse as Message>::Result;

    fn handle(&mut self, msg: DelVnByTitleResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelVnByTitleResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let search_vn = actors::db::SearchVn(title.clone());
        let search_vn = self.db.send(search_vn).into_actor(self);
        let search_vn = search_vn.map(move |result, _act, ctx| match result {
            Ok(vns) => match vns.len() {
                0 => ctx.notify(DelVnByExactVndbTitleResponse::new(target, from, is_pm, command::DelVnByExactVndbTitle { title })),
                1 => {
                    let vn = unsafe { vns.get_unchecked(0) };
                    ctx.notify(DelVnByIdResponse::new(target, from, is_pm, command::DelVnById { id: vn.id as u64 }))
                },
                num => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::better_vn_query(num, &title))),
            },
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error))),
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelVn: {}", error)
        });
        ctx.spawn(search_vn);

        Ok(())
    }
}

type DelVnByIdResponse = GetIrcResponse<command::DelVnById>;
impl Handler<DelVnByIdResponse> for Irc {
    type Result = <DelVnByIdResponse as Message>::Result;

    fn handle(&mut self, msg: DelVnByIdResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelVnByIdResponse {target, from, is_pm, cmd} = msg;
        let id = cmd.id;

        let del_vn = actors::db::DelVnData(id);
        let del_vn = self.db.send(del_vn).into_actor(self);
        let del_vn = del_vn.map(move |result, _act, ctx| match result {
            Ok(_) => {
                let text = format!("Removed v{} from DB", id);
                ctx.notify(TextResponse::new(target, from, is_pm, text.into()));
            },
            Err(error) => {
                error!("DB error: {}", error);
                ctx.notify(TextResponse::new(target, from, is_pm, command::Text::error(error)))
            },
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelVnById: {}", error)
        });
        ctx.spawn(del_vn);

        Ok(())
    }
}

//References
type GetRefResponse = GetIrcResponse<command::Ref>;
impl Handler<GetRefResponse> for Irc {
    type Result = <GetRefResponse as Message>::Result;

    fn handle(&mut self, msg: GetRefResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetRefResponse {target, from, is_pm, cmd} = msg;
        let command::Ref {kind, id, url} = cmd;

        let get_ref = actors::vndb::Get::get_by_id(kind.clone(), id);
        let get_ref = self.vndb.send(get_ref.into()).into_actor(self);
        let get_ref = get_ref.map(move |result, _act, ctx| match result {
            Ok(results) => match results {
                actors::vndb::Response::Results(result) => {
                    let items = try_option!(result.get("items"), "VNDB results is missing items field!");
                    let item = try_option!(items.get(0), "VNDB results's items is empty field!");
                    let name = try_option!(item.get("title").or(item.get("name")).or(item.get("username")),
                                                "VNDB results's item is missing title/name field!");

                    let kind = kind.short();
                    let id = item.get("id").unwrap();
                    let text = match url {
                        true => format!("{0}{1}: {2} - https://vndb.org/{0}{1}", kind, id, name),
                        false => format!("{0}{1}: {2}", kind, id, name),
                    };
                    ctx.notify(TextResponse::new(target, from, is_pm, text.into()))
                },
                other => {
                    error!("Unexpected VNDB response on get: {:?}", other);
                    ctx.notify(TextResponse::new(target, from, is_pm, command::Text::bad_vndb()))
                }
            },
            Err(error) => {
                warn!("GetRefResponse Error: '{}'. Re-try", error);
                let cmd = command::Ref {id, kind, url};
                ctx.notify_later(GetRefResponse::new(target, from, is_pm, cmd), duration::ms(CMD_DELAY_MS));
            }
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetRef: {}", error)
        });

        ctx.spawn(get_ref);

        Ok(())
    }
}

impl Supervised for Irc {
    fn restarting(&mut self, _: &mut Context<Self>) {
        info!("IRC: Restarting...");
        self.client.take();
    }
}

impl Actor for Irc {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        const TIMEOUT_MS: u64 = 1500;

        let future: IrcClientFuture<'static> = match IrcClient::new_future(Arbiter::handle().clone(), &self.config) {
            Ok(result) => unsafe { ::mem::transmute(result) },
            Err(error) => {
                error!("IRC: Unable to create new client. Error: {}", error);
                ctx.run_later(duration::ms(TIMEOUT_MS), |_, ctx| ctx.stop());
                return;
            }
        };

        future.into_actor(self).map_err(|error, _act, ctx| {
            error!("IRC: Unable to connect to server. Error: {}", error);
            ctx.run_later(duration::ms(TIMEOUT_MS), |_, ctx| ctx.stop());
        }).map(|packed, act, ctx| {
            info!("IRC: Connected");
            let PackedIrcClient(client, future) = packed;

            let future = future.into_actor(act).map_err(|error, _act, ctx| {
                error!("IRC: Runtime error: {}", error);
                ctx.stop();
            });
            ctx.spawn(future);

            info!("IRC: Started");
            client.send_cap_req(&[irc::proto::caps::Capability::MultiPrefix]).expect("To send caps");
            client.identify().expect("To identify");

            let stream = client.stream().map(|msg| IrcMessage(msg));
            ctx.add_stream(stream);
            act.client = Some(client);
        }).wait(ctx);
    }
}
