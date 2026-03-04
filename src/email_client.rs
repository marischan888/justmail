// http client for Rest Api
use reqwest::Client;
use secrecy::{ExposeSecret, SecretBox};
use crate::domain::SubscriberEmail;

pub struct EmailClient {
    http_client: Client,
    base_url: String,
    sender_email: SubscriberEmail,
    auth_token: SecretBox<String>,
}

impl EmailClient {
    pub fn new(base_url: String,
               sender_email: SubscriberEmail,
               auth_token: SecretBox<String>,
               timeout: std::time::Duration
    ) -> Self {
        let http_client = Client::builder()
            .timeout(timeout)
            .build()
            .unwrap();
        Self {
            http_client,
            base_url,
            sender_email,
            auth_token,
        }
    }

    pub async fn send_email(
        &self,
        receiver_email: SubscriberEmail,
        subject: &str,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), reqwest::Error> {
        let url = format!("{}/email", self.base_url);
        let request_body = SendEmailRequest {
            from: self.sender_email.as_ref(),
            to: receiver_email.as_ref(),
            subject,
            html_body: html_content,
            text_body: text_content,
        };
        self.http_client
            .post(&url)
            .header(
                "X-Postmark-Server-Token",
                self.auth_token.expose_secret(),
            )
            .json(&request_body)
            .send()
            .await?
            .error_for_status()?;  // here is the http response error handler
        Ok(())
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct SendEmailRequest<'a> {
    from: &'a str,
    to: &'a str,
    subject: &'a str,
    html_body: &'a str,
    text_body: &'a str,
}
#[cfg(test)]
mod tests {
    use claim::{assert_err, assert_ok};
    use crate::domain::SubscriberEmail;
    use crate::email_client::{EmailClient};
    use fake::faker::internet::en::SafeEmail;
    use fake::faker::lorem::en::{Paragraph, Sentence};
    use fake::{Faker, Fake};
    use secrecy::{SecretBox};
    use wiremock::matchers::{header_exists, header, path, method, any};
    use wiremock::{Mock, MockServer, ResponseTemplate, Request, Match};

    struct SendEmailBodyMatcher;

    impl Match for SendEmailBodyMatcher {
        fn matches(&self, request: &Request) -> bool {
            let result: Result<serde_json::Value, _> = serde_json::from_slice(&request.body);
            if let Ok(body) = result {
                // dbg!(&body); -- body inspection by running cargo test send_email
                body.get("From").is_some()
                    && body.get("To").is_some()
                    && body.get("Subject").is_some()
                    && body.get("HtmlBody").is_some()
                    && body.get("TextBody").is_some()
            } else {
                false
            }
        }
    }

    // test "sending request" and "a valid resonse"
    #[tokio::test]
    async fn send_email_send_the_expected_request() {
        // Arrange
        // mock http response
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri(), email());

        Mock::given(header_exists("X-Postmark-Server-Token"))
            .and(header("Content-Type", "application/json"))
            .and(path("/email"))
            .and(method("POST"))
            .and(SendEmailBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)// mock expectation
            .mount(&mock_server)
            .await;
        // Act
        let response = email_client
            .send_email(email(), &subject(), &content(), &content())
            .await;
        // Assert
        assert_ok!(response);
    }

    #[tokio::test]
    async fn send_email_fails_with_response_500() {
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri(), email());

        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(1)// mock expectation
            .mount(&mock_server)
            .await;

        let response = email_client
            .send_email(email(), &subject(), &content(), &content())
            .await;
        assert_err!(response);
    }

    #[tokio::test]
    async fn send_email_times_out_if_the_server_takes_too_long() {
        let mock_server = MockServer::start().await;
        let email_client = email_client(mock_server.uri(), email());
        let mock_response = ResponseTemplate::new(200)
            .set_delay(std::time::Duration::from_secs(180));

        Mock::given(any())
            .respond_with(mock_response)
            .expect(1)// mock expectation
            .mount(&mock_server)
            .await;

        let response = email_client
            .send_email(email(), &subject(), &content(), &content())
            .await;
        assert_err!(response);
    }

    fn subject() -> String {
        Sentence(1..2).fake()
    }

    fn content() -> String {
        Paragraph(1..10).fake()
    }

    fn email() -> SubscriberEmail {
        SubscriberEmail::parse(SafeEmail().fake()).unwrap()
    }

    fn email_client(url: String, sender: SubscriberEmail) -> EmailClient {
        EmailClient::new(
            url,
            sender,
            SecretBox::new(Faker.fake()),
            std::time::Duration::from_millis(200),
        )
    }
}