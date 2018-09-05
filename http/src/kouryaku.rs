use ::actix::prelude::{Actor, Supervised, Handler, Context, SystemService, Message, ActorFuture, WrapFuture, ResponseActFuture};
use ::regex::Regex;

use ::futures::{future, Future};
use ::encoding::codec::japanese::Windows31JEncoding as ShiftJS;
use ::encoding::types::{Encoding, DecoderTrap};
use ::{HttpDate, ResponseError, Request, IfModifiedSince, AutoClient};

const WALK_ROOT: &'static str = "http://seiya-saiga.com/game/";
const WALK_PAGE: &'static str = "http://seiya-saiga.com/game/kouryaku.html";

pub struct Entry {
    pub title: String,
    pub uri: String
}

impl Entry {
    fn new(title: String, uri: String) -> Self {
        Self {
            title: title.to_lowercase(),
            uri
        }
    }
}

struct Cache {
    date: HttpDate,
    content: Vec<Entry>
}

///Provides Kouryaku getter service
pub struct Kouryaku {
    cache: Option<Cache>,
    regex: Regex
}

impl Kouryaku {
    fn find_iter(&self, title: &str) -> Option<impl Iterator<Item=&Entry>> {
        match self.cache.as_ref() {
            Some(cache) => {
                let title = title.to_lowercase();
                Some(cache.content.iter().filter(move |entry| entry.title.contains(&title)))
            },
            None => None
        }
    }

    fn update_cache(&mut self, html: String, last_modified: HttpDate) {
        let mut entries = Vec::with_capacity(1024);
        for caps in self.regex.captures_iter(&html) {
            let entry = Entry::new(caps["title"].to_string(), caps["url"].to_string());
            entries.push(entry);
        }

        self.cache = Some(Cache {
            date: last_modified,
            content: entries
        });
    }

    fn prepare_request(&self) -> Request {
        match self.cache.as_ref() {
            Some(cache) => Request::get(WALK_PAGE).expect("Create request").set_date(&cache.date, IfModifiedSince).empty(),
            None => Request::get(WALK_PAGE).expect("Create request").empty(),
        }
    }

    fn request_kor(&self) -> impl Future<Item=Option<(String, HttpDate)>, Error=String> {
        self.prepare_request()
            .send()
            .map_err(|error| match error {
                ResponseError::Timeout(_) => format!("Request timedout"),
                ResponseError::Timer(_, _) => format!("Request timedout"),
                ResponseError::HyperError(error) => format!("Request failed. Error: {}", error),
            })
            .and_then(|rsp| -> Box<Future<Item=Option<(String, HttpDate)>, Error=String>> {
                if rsp.is_redirect() {
                    Box::new(future::ok(None))
                } else if rsp.is_success() {
                    let last_modified = rsp.last_modified().expect("To have Last-Modified");

                    let res = rsp.body().limit(u64::max_value())
                                        .map_err(|error| {
                                            warn!("HTTP: Error while reading body: {:?}", error);
                                            "Unable to read HTTP body from Kouryaku".to_string()
                                        }).and_then(move |body| {
                                            ShiftJS.decode(&body, DecoderTrap::Strict).map_err(|error| {
                                                warn!("HTTP: Unable to decode using ShiftJS: {:?}", error);
                                                "Kouryaku has invalid encoding".to_string()
                                            }).map(|text| Some((text, last_modified)))
                                        });
                    Box::new(res)
                } else {
                    Box::new(future::err(format!("Request failed. Status: {}", rsp.status())))
                }
            })
    }
}

impl Default for Kouryaku {
    fn default() -> Self {
        Self {
            cache: None,
            regex: Regex::new("<td align=\"left\"><B><A href=\"(?P<url>[^\"]+)\">(?P<title>[^<]+)</A></B></td>").expect("To create regex")
        }
    }
}

impl Actor for Kouryaku {
    type Context = Context<Self>;
}

impl Supervised for Kouryaku {}
impl SystemService for Kouryaku {}

///Looks up single kouryaku in service
pub struct Find(pub String);

impl Message for Find {
    type Result = Result<Option<(String, String)>, String>;
}

impl Handler<Find> for Kouryaku {
    type Result = ResponseActFuture<Self, Option<(String, String)>, String>;

    fn handle(&mut self, msg: Find, _: &mut Self::Context) -> Self::Result {
        let title = msg.0;
        let req = self.request_kor()
                      .into_actor(self)
                      .map(|new_entry, act, _ctx| match new_entry {
                          Some((new, last_modified)) => {
                              act.update_cache(new, last_modified);
                          },
                          None => ()
                      }).map(move |_, act, _ctx| {
                          act.find_iter(&title)
                             .and_then(|mut iter| {
                                iter.next().map(|entry| (entry.title.clone(), format!("{}{}", WALK_ROOT, &entry.uri)))
                             })
                      });

        Box::new(req)
    }
}

#[cfg(test)]
mod tests {
    use super::{Kouryaku, Find, Future};
    use ::init;
    use ::actix::{spawn, System};

    #[test]
    fn test_kouryaku() {
        const MIRAI_RADIO: &'static str = "未来ラジオと人工";
        const DRAC: &'static str = "ドラクリウス";


        System::run(|| {
            init();

            let kouryaku = System::current().registry().get::<Kouryaku>();

            let drac = kouryaku.send(Find(DRAC.to_string())).map(move |drac| {
                let drac = drac.expect("Expect to successfully find Duraculis");
                let drac = drac.expect("Expect to find some Duraculis");

                assert_eq!(drac.0, "ドラクリウス");
                assert_eq!(drac.1, "http://seiya-saiga.com/game/meroq/drac.html");

                System::current().stop();
            }).map_err(|_| panic!("Unable to send msg!"));

            let mirai = kouryaku.send(Find(MIRAI_RADIO.to_string())).map(move |mirai_radio| {
                let mirai_radio = mirai_radio.expect("Expect to successfully find Mirai Radio");
                let mirai_radio = mirai_radio.expect("Expect to find some Mirai Radio");

                assert_eq!(mirai_radio.0, "未来ラジオと人工鳩");
                assert_eq!(mirai_radio.1, "http://seiya-saiga.com/game/laplacian/mirairadio.html");
                spawn(drac);
            }).map_err(|_| panic!("Unable to send msg!"));

            spawn(mirai);
        });
    }
}
