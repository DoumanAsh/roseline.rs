extern crate futures;

use futures::future::Future;

use ::irc::client::server::IrcServer;
use ::irc::client::server::utils::ServerExt;

use ::irc::proto::message::Message;
use ::irc::proto::command::Command;

use ::vndb;
use ::db::Db;

type ReturnFuture = Box<Future<Item=(), Error=::irc::error::Error>>;

#[inline]
pub fn future_ok() -> futures::future::FutureResult<(), ::irc::error::Error> {
    futures::future::ok(())
}

mod args;
mod command;

pub struct MessageHandler {
    server: IrcServer,
    vndb: vndb::Client,
    db: Db
}

impl MessageHandler {
    pub fn new(server: IrcServer, vndb: vndb::Client, db: Db) -> Self {
        Self {
            server,
            vndb,
            db
        }
    }

    pub fn dispatch(&self, message: Message) -> ReturnFuture {
        let from = message.prefix.as_ref().map(|prefix| &prefix[..prefix.find('!').unwrap_or(0)]);

        match message.command {
            Command::PRIVMSG(target, msg) => self.privmsg(from, target, msg),
            Command::JOIN(chanlist, _, _) => self.join(chanlist, from),
            Command::PART(chanlist, _) => self.part(chanlist, from),
            Command::KICK(chanlist, user, _) => self.kick(chanlist, user, from),
            message => self.unhandled(from, message)
        }
    }

    //Dispatchers internal methods
    #[inline(always)]
    ///Returns whether message is sent privately to bot.
    fn is_private(&self, target: &str) -> bool {
        self.server.current_nickname() == target
    }

    //Dispatchers
    #[inline(always)]
    fn privmsg(&self, from: Option<&str>, target: String, msg: String) -> ReturnFuture {
        info!("from {:?} to {}: {}", from, target, msg);

        let cmd = match command::Command::from_str(&msg) {
            Some(cmd) => cmd,
            None => return Box::new(future_ok())
        };

        let server = self.server.clone();
        let from = from.unwrap().to_string();
        let is_private = self.is_private(&target);
        let cmd = cmd.exec(self.vndb.clone(), self.db.clone())
                     .then(move |result| {
                         let result = match result {
                             Ok(command::CommandResult::Single(result)) => vec![result],
                             Ok(command::CommandResult::Multi(result)) => result,
                             Err(error) => vec![format!("Some error happened: {}", error)]
                         };

                         match is_private {
                             true => for item in result {
                                 match server.send_privmsg(&from, &format!("{}", item)) {
                                     Ok(_) => (),
                                     Err(error) => return Err(error),
                                 }
                             },
                             false => for item in result {
                                 match server.send_privmsg(&target, &format!("{}: {}", from, item)) {
                                     Ok(_) => (),
                                     Err(error) => return Err(error),
                                 }
                             },
                         }

                         Ok(())
                     });
        Box::new(cmd)
    }

    #[inline(always)]
    fn join(&self, chanlist: String, from: Option<&str>) -> ReturnFuture {
        info!("{:?} joined {}", from, chanlist);
        Box::new(future_ok())
    }

    #[inline(always)]
    fn part(&self, chanlist: String, from: Option<&str>) -> ReturnFuture {
        info!("{:?} left {}", from, chanlist);
        Box::new(future_ok())
    }

    #[inline(always)]
    fn kick(&self, chanlist: String, who: String, from: Option<&str>) -> ReturnFuture {
        info!("{:?} kicked {} out of {}", from, who, chanlist);
        if who == self.server.current_nickname() {
            let result = self.server.send_join(&chanlist);
            Box::new(futures::future::result(result))
        }
        else  {
            Box::new(future_ok())
        }
    }

    #[inline(always)]
    fn unhandled(&self, _from: Option<&str>, message: Command) -> ReturnFuture {
        trace!("Unhandled message={:?}", message);
        Box::new(future_ok())
    }
}
