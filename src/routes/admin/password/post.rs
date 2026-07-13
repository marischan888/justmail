use actix_web::{HttpResponse, web};
use actix_web_flash_messages::FlashMessage;
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use validator::ValidateLength;

use crate::authentication::{AuthError, Credentials, validate_credentials};
use crate::routes::admin::dashboard::get_username;
use crate::session_state::TypedSession;
use crate::utils::{e500, see_other};


#[derive(serde::Deserialize)]
pub struct FormData {
    current_password: SecretString,
    new_password: SecretString,
    check_password: SecretString,
}

#[tracing::instrument
(
    skip(form, session, pool),
    fields(
        current_password=tracing::field::Empty,
        new_password=tracing::field::Empty,
        check_password=tracing::field::Empty,
    ),
)]
pub async fn change_password(
    form: web::Form<FormData>,
    session: TypedSession,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error>{
    // TODO refeactor using match?
    let user_id = session.get_user_id().map_err(e500)?;
    if user_id.is_none() {
        return Ok(see_other("/login"));
    };
    // distinct new password
    if form.new_password.expose_secret() != form.check_password.expose_secret() {
        FlashMessage::error("You enter two different new password - this field value must be the same").send();
        return Ok(see_other("/admin/password"));
    };
    // short new password
    if ! ValidateLength::validate_length(&form.new_password.expose_secret(), Some(12), Some(128), None) {
        FlashMessage::error("Password should be longer than 12 characters but shorter than 128 characters.").send();
        return Ok(see_other("/admin/password"));
    }

    let user_id = user_id.unwrap();
    let user_name = get_username(user_id, &pool).await.map_err(e500)?;
    let credential = Credentials {
        username: user_name,
        password: form.0.current_password,
    };

    if let Err(e) = validate_credentials(credential, &pool).await {
        return match e {
            AuthError::InvalidCredentials(_) => {
                FlashMessage::error("Wrong current password").send();
                Ok(see_other("/admin/password"))
            }
            AuthError::UnexpectedError(_) => Err(e500(e).into()),
        }
    }
    
    todo!()
}
