// http client for Rest Api
use reqwest::Client;
use crate::domain::SubscriberEmail;

pub struct EmailClient {
    http_client: Client,
    base_url: String,
    email: SubscriberEmail,
}

impl EmailClient {
    pub fn new(base_url: String, email: SubscriberEmail) -> Self {
        Self {
            http_client: Client::new(),
            base_url,
            email,
        }
    }
    
    pub async fn send_email(
        &self,
        recipient: SubscriberEmail,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), String> {
        todo!()
    }
}