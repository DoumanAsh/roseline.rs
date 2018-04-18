extern crate actix_web;

use self::actix_web::{
    HttpResponse
};

#[derive(Serialize)]
pub struct ClientError {
    message: &'static str
}

impl ClientError {
    pub fn new(message: &'static str) -> Self {
        Self {
            message
        }
    }
}

impl From<ClientError> for HttpResponse {
    fn from(error: ClientError) -> Self {
        HttpResponse::BadRequest().json(error)
    }
}
