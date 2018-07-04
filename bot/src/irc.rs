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
    IrcClient
};
use self::irc::proto::command::Command;
use self::irc::proto::message::Message as InnerIrcMessage;
use self::irc::error::IrcError;
use self::irc::client::ext::ClientExt;
use self::utils::duration;

use ::collections::HashSet;

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

pub struct Irc {
    config: Config,
    handler: Addr<actors::exec::Executor>,
    client: Option<IrcClient>,
    ignores: HashSet<String>
}

impl Irc {
    pub fn new(config: Config, handler: Addr<actors::exec::Executor>) -> Self {
        let mut ignores = HashSet::new();
        ignores.insert("Fltrsh".to_string());

        Self {
            config,
            handler,
            client: None,
            ignores
        }
    }
}

#[derive(Debug)]
struct IrcMessage(pub InnerIrcMessage);

impl Message for IrcMessage {
    type Result = Result<(), IrcError>;
}

impl StreamHandler<IrcMessage, IrcError> for Irc {
    fn error(&mut self, error: IrcError, _ctx: &mut Self::Context) -> actix::Running {
        warn!("VNDB: IO error: {}", error);
        actix::Running::Stop
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        warn!("VNDB: Connection is closed");
        ctx.stop();
    }

    fn handle(&mut self, msg: IrcMessage, ctx: &mut Self::Context) {
        debug!("IRC: message={:?}", msg);

        let msg = msg.0;
        let from = msg.prefix.as_ref().map(|prefix| &prefix[..prefix.find('!').unwrap_or(0)]);
        let client = self.client.as_ref().unwrap();

        match msg.command {
            Command::PRIVMSG(target, msg) => {
                debug!("from {:?} to {}: {}", from, target, msg);

                let from = from.unwrap().to_string();

                if self.ignores.contains(&from) {
                    return;
                }

                let cmd = match command::Command::from_str(&msg) {
                    Some(cmd) => cmd,
                    None => return
                };

                let is_pm = *client.current_nickname() == target;

                match cmd {
                    command::Command::Text(text) => ctx.notify(TextResponse::new(target, from, is_pm, text)),
                    command::Command::GetVn(get_vn) => ctx.notify(GetVnResponse::new(target, from, is_pm, get_vn)),
                    command::Command::GetHook(get_hook) => ctx.notify(GetHookResponse::new(target, from, is_pm, get_hook)),
                    command::Command::SetHook(set_hook) => ctx.notify(SetHookResponse::new(target, from, is_pm, set_hook)),
                    command::Command::DelHook(del_hook) => ctx.notify(DelHookResponse::new(target, from, is_pm, del_hook)),
                    command::Command::DelVn(del_vn) => ctx.notify(DelVnResponse::new(target, from, is_pm, del_vn)),
                    command::Command::Ignore(name) => match self.ignores.contains(&name) {
                        true => {
                            let text = format!("Removed '{}' from ignore list", name);
                            self.ignores.remove(&name);
                            info!("{}", &text);
                            ctx.notify(TextResponse::new(target, from, is_pm, text.into()))
                        },
                        false => {
                            let text = format!("Added '{}' to ignore list", name);
                            self.ignores.insert(name);
                            info!("{}", &text);
                            ctx.notify(TextResponse::new(target, from, is_pm, text.into()))
                        }
                    },
                    command::Command::IgnoreList => {
                        let list = self.ignores.iter().map(String::as_str).collect::<Vec<_>>();
                        let list = &list[..];
                        let text = format!("Ignore list: {}", list.join(", "));
                        ctx.notify(TextResponse::new(target, from, is_pm, text.into()))
                    },
                    command::Command::Refs(mut refs) => for reference in &mut refs.refs {
                        let reference = reference.take();
                        if let Some(reference) = reference {
                            ctx.notify(GetRefResponse::new(target.clone(), from.clone(), is_pm, reference));
                        }
                        else {
                            break;
                        }
                    },
                    command::Command::Shutdown => {
                        ctx.notify(StopSystem);
                    },
                }
            },
            Command::JOIN(chanlist, _, _) => debug!("{:?} joined {}", from, chanlist),
            Command::PART(chanlist, _) => debug!("{:?} left {}", from, chanlist),
            Command::KICK(chanlist, user, _) => {
                debug!("{:?} kicked {} out of {}", from, user, chanlist);
                if user == *client.current_nickname() {
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

        let get_vn = actors::exec::FindVn::new(title);
        let get_vn = self.handler.send(get_vn).into_actor(self);
        let get_vn = get_vn.map(move |result, _act, ctx| match result {
            Ok(vn) => {
                let text = format!("{} - https://vndb.org/v{}", vn.title.as_ref().unwrap(), vn.id);
                ctx.notify(TextResponse::new(target, from, is_pm, text.into()))
            },
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}", error).into()))
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetVn: {}", error);
        });
        ctx.spawn(get_vn);

        Ok(())
    }
}

//.set_hook
type SetHookResponse = GetIrcResponse<command::SetHook>;
impl Handler<SetHookResponse> for Irc {
    type Result = <SetHookResponse as Message>::Result;

    fn handle(&mut self, msg: SetHookResponse, ctx: &mut Self::Context) -> Self::Result {
        let SetHookResponse {target, from, is_pm, cmd} = msg;
        let command::SetHook {title, version, code} = cmd;

        let set_hook = actors::exec::SetHook::new(title.clone(), version, code);
        let set_hook = self.handler.send(set_hook).into_actor(self);
        let set_hook = set_hook.map(move |result, _act, ctx| match result {
            Ok(hook) => ctx.notify(TextResponse::new(target, from, is_pm, format!("Added hook '{}' for VN: {}", hook.code, title).into())),
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}", error).into()))
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing SetHook: {}", error)
        });
        ctx.spawn(set_hook);

        Ok(())
    }
}

//.hook
type GetHookResponse = GetIrcResponse<command::GetHook>;
impl Handler<GetHookResponse> for Irc {
    type Result = <GetHookResponse as Message>::Result;

    fn handle(&mut self, msg: GetHookResponse, ctx: &mut Self::Context) -> Self::Result {
        let GetHookResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let get_hook = actors::exec::GetHook(title);
        let get_hook = self.handler.send(get_hook).into_actor(self);
        let get_hook = get_hook.map(move |result, _act, ctx| match result {
            Ok(data) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}", data).into())),
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}", error).into()))
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetHook: {}", error)
        });
        ctx.spawn(get_hook);

        Ok(())
    }
}

//.del_hook
type DelHookResponse = GetIrcResponse<command::DelHook>;
impl Handler<DelHookResponse> for Irc {
    type Result = <DelHookResponse as Message>::Result;

    fn handle(&mut self, msg: DelHookResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelHookResponse {target, from, is_pm, cmd} = msg;
        let command::DelHook {title, version} = cmd;

        let del_hook = actors::exec::DelHook::new(title.clone(), version);
        let del_hook = self.handler.send(del_hook).into_actor(self);
        let del_hook = del_hook.map(move |result, _act, ctx| match result {
            Ok(0) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}: No hook to remove.", title).into())),
            Ok(_) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}: Removed hook.", title).into())),
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}", error).into()))
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelHook: {}", error)
        });
        ctx.spawn(del_hook);

        Ok(())
    }
}

//.del_vn
type DelVnResponse = GetIrcResponse<command::DelVn>;
impl Handler<DelVnResponse> for Irc {
    type Result = <DelVnResponse as Message>::Result;

    fn handle(&mut self, msg: DelVnResponse, ctx: &mut Self::Context) -> Self::Result {
        let DelVnResponse {target, from, is_pm, cmd} = msg;
        let title = cmd.title;

        let del_vn = actors::exec::DelVn(title.clone());
        let del_vn = self.handler.send(del_vn).into_actor(self);
        let del_vn = del_vn.map(move |result, _act, ctx| match result {
            Ok(0) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}: No such VN exists in DB.", title).into())),
            Ok(_) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}: removed from DB.", title).into())),
            Err(error) => ctx.notify(TextResponse::new(target, from, is_pm, format!("{}", error).into()))
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing DelVn: {}", error)
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

        let get_ref = actors::exec::GetVndbObject::new(id, kind.clone());
        let get_ref = self.handler.send(get_ref).into_actor(self);
        let get_ref = get_ref.map(move |result, _act, ctx| match result {
            Ok(result) => {
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
            Err(error) => warn!("GetRef failed: {}", error)
        }).map_err(|error, _act, _ctx| {
            error!("IRC: error processing GetRef: {}", error)
        });

        ctx.spawn(get_ref);

        Ok(())
    }
}

pub struct StopSystem;
impl Message for StopSystem {
    type Result = ();
}

impl Handler<StopSystem> for Irc {
    type Result = ();

    fn handle(&mut self, _msg: StopSystem, _ctx: &mut Self::Context) -> Self::Result {
        warn!("Shutdown command is issued!");

        let client = self.client.as_ref().unwrap();

        if let Some(chanlist) = client.list_channels() {
            if chanlist.len() > 0 {
                debug!("Leaving chanlist={:?}", &chanlist);
                let _ = client.send_part(&chanlist.join(","));
            }
        };

        System::current().stop();
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
        info!("IRC: starting");

        let irc = IrcClient::new_future(self.config.clone()).into_actor(self).map_err(|error, _act, ctx| {
            error!("IRC: Unable to connect to server. Error: {}", error);
            ctx.run_later(duration::ms(TIMEOUT_MS), |_, ctx| ctx.stop());
        }).map(|(client, future), act, ctx| {
            info!("IRC: Connected");
            let future = future.into_actor(act).map_err(|error, _act, ctx| {
                error!("IRC: Runtime error: {}", error);
                ctx.stop();
            });
            ctx.spawn(future);

            client.send_cap_req(&[irc::proto::caps::Capability::MultiPrefix]).expect("To send caps");
            client.identify().expect("To identify");

            let stream = client.stream().map(|msg| IrcMessage(msg));
            Self::add_stream(stream, ctx);

            info!("IRC: Joined");
            act.client = Some(client);
        });

        ctx.spawn(irc);
    }
}
