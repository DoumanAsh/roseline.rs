extern crate futures;
extern crate actix;
extern crate actors;

extern crate typemap;
extern crate serenity;

use self::serenity::client::Client;
use self::serenity::prelude::{EventHandler, Context};
use self::serenity::framework::standard::{
    StandardFramework,
    CommandError,
    Args,
    help_commands,
    HelpBehaviour
};
use self::serenity::model::channel::Message;
use self::futures::Future;

use ::command;

struct CommandHandler;
impl typemap::Key for CommandHandler {
    type Value = actix::Addr<actors::exec::Executor>;
}

struct Handler;

impl EventHandler for Handler {
    fn message(&self, _: Context, msg: Message) {
        //We ignore bot messages
        if msg.author.bot == true {
            return;
        }

        let chan = msg.channel_id.name().unwrap_or("{}".to_string());

        debug!("{}|{}: {}", chan, msg.author.name, msg.content);
    }
}

pub fn client(executor: actix::Addr<actors::exec::Executor>) -> Client {
    let token = include_str!("../discord.token");
    // Login with a bot token from the environment
    let mut client = Client::new(token, Handler).expect("Error creating client");
    {
        let mut data = client.data.lock();
        data.insert::<CommandHandler>(executor);
    }

    let framework = StandardFramework::new().configure(|c| c.prefix(".").ignore_bots(true).case_insensitivity(true).allow_dm(true))
                                            .customised_help(help_commands::plain, |c| {
                                                c.lacking_permissions(HelpBehaviour::Hide)
                                            })
                                            .command("ping", |config| config.desc("Ping").exec(ping))
                                            .command("vn", |config| config.desc("Search VN").exec(vn))
                                            .command("hook", |config| config.desc("Get Hook for VN").exec(hook))
                                            .command("del_hook", |config| {
                                                config.desc("Remove Hook for VN")
                                                      .exec(del_hook)
                                                      .usage(command::DEL_HOOK_USAGE)
                                            })
                                            .command("set_hook", |config| {
                                                config.desc("Set Hook for VN")
                                                      .usage(command::SET_HOOK_USAGE)
                                                      .exec(set_hook)
                                            })
                                            .command("del_vn", |config| config.desc("Remove VN").exec(del_vn));

    client.with_framework(framework);

    client
}

fn ping(_context: &mut Context, message: &Message, _args: Args) -> Result<(), CommandError> {
    let _ = message.reply("pong")?;

    Ok(())
}

fn vn(context: &mut Context, message: &Message, args: Args) -> Result<(), CommandError> {
    let executor = {
        let data = context.data.lock();
        data.get::<CommandHandler>().unwrap().clone()
    };

    let get_vn = actors::exec::FindVn::new(args.full().to_string());
    let result = executor.send(get_vn).wait().map_err(|error| CommandError(format!("{}", error)))?;

    match result {
        Ok(vn) => {
            let text = format!("{} - https://vndb.org/v{}", vn.title.as_ref().unwrap(), vn.id);
            message.reply(&text)?;
        },
        Err(error) => {
            message.reply(&format!("{}", error))?;
        }
    }

    Ok(())
}

fn hook(context: &mut Context, message: &Message, args: Args) -> Result<(), CommandError> {
    if args.is_empty() {
        message.reply("For which VN?")?;
    }
    else {
        let executor = {
            let data = context.data.lock();
            data.get::<CommandHandler>().unwrap().clone()
        };

        let get_hook = actors::exec::GetHook(args.full().to_string());
        let result = executor.send(get_hook).wait().map_err(|error| CommandError(format!("{}", error)))?;

        match result {
            Ok(data) => message.reply(&format!("{}", data))?,
            Err(error) => message.reply(&format!("{}", error))?
        };
    }

    Ok(())
}

fn set_hook(context: &mut Context, message: &Message, args: Args) -> Result<(), CommandError> {
    let mut args = args.multiple_quoted()?;

    if args.len() == 0 {
        message.reply(command::SET_HOOK_USAGE)?;
    }
    else if args.len() != 3 {
        message.reply("Wrong number of arguments. Expected 3. See usage.")?;
    }
    else {
        let executor = {
            let data = context.data.lock();
            data.get::<CommandHandler>().unwrap().clone()
        };

        let mut args = args.drain(..);
        let title: String = args.next().unwrap();
        let version = args.next().unwrap();
        let code = args.next().unwrap();

        let set_hook = actors::exec::SetHook::new(title.clone(), version, code);
        let result = executor.send(set_hook).wait().map_err(|error| CommandError(format!("{}", error)))?;

        match result {
            Ok(hook) => message.reply(&format!("Added hook '{}' for VN: {}", hook.code, title))?,
            Err(error) => message.reply(&format!("{}", error))?
        };
    }

    Ok(())
}

fn del_hook(context: &mut Context, message: &Message, args: Args) -> Result<(), CommandError> {
    let mut args = args.multiple_quoted()?;

    if args.len() == 0 {
        message.reply(command::DEL_HOOK_USAGE)?;
    }
    else if args.len() != 2 {
        message.reply("Wrong number of arguments. Expected 2. See usage.")?;
    }
    else {
        let executor = {
            let data = context.data.lock();
            data.get::<CommandHandler>().unwrap().clone()
        };

        let mut args = args.drain(..);
        let title: String = args.next().unwrap();
        let version = args.next().unwrap();

        let del_hook = actors::exec::DelHook::new(title.clone(), version);
        let result = executor.send(del_hook).wait().map_err(|error| CommandError(format!("{}", error)))?;

        match result {
            Ok(0) => message.reply(&format!("{}: No hook to remove.", title))?,
            Ok(_) => message.reply(&format!("{}: Removed hook.", title))?,
            Err(error) => message.reply(&format!("{}", error))?
        };
    }

    Ok(())
}

fn del_vn(context: &mut Context, message: &Message, args: Args) -> Result<(), CommandError> {
    if args.is_empty() {
        message.reply("For which VN?")?;
    }
    else {
        let executor = {
            let data = context.data.lock();
            data.get::<CommandHandler>().unwrap().clone()
        };

        let del_vn = actors::exec::DelVn(args.full().to_string());
        let result = executor.send(del_vn).wait().map_err(|error| CommandError(format!("{}", error)))?;

        match result {
            Ok(0) => message.reply(&format!("{}: No such VN exists in DB.", args.full()))?,
            Ok(_) => message.reply(&format!("{}: removed from DB.", args.full()))?,
            Err(error) => message.reply(&format!("{}", error))?
        };
    }

    Ok(())

}
