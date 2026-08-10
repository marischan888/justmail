use actix_web::{HttpResponse, web};
use actix_web_flash_messages::FlashMessage;
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use validator::ValidateLength;

use crate::authentication::{AuthError, Credentials, UserId, validate_credentials};
use crate::routes::admin::dashboard::get_username;
use crate::utils::{e500, see_other};

#[derive(serde::Deserialize)]
pub struct FormData {
    current_password: SecretString,
    new_password: SecretString,
    check_password: SecretString,
}

#[tracing::instrument
(
    skip(form, pool, user_id),
    fields(
        current_password=tracing::field::Empty,
        new_password=tracing::field::Empty,
        check_password=tracing::field::Empty,
    ),
)]
pub async fn change_password(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();
    // distinct new password
    if form.new_password.expose_secret() != form.check_password.expose_secret() {
        FlashMessage::error(
            "You enter two different new password - this field value must be the same",
        )
        .send();
        return Ok(see_other("/admin/password"));
    };
    // short new password
    if !ValidateLength::validate_length(
        &form.new_password.expose_secret(),
        Some(12),
        Some(128),
        None,
    ) {
        FlashMessage::error(
            "Password should be longer than 12 characters but shorter than 128 characters.",
        )
        .send();
        return Ok(see_other("/admin/password"));
    }

    let user_name = get_username(*user_id, &pool).await.map_err(e500)?;
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
        };
    }

    crate::authentication::insert_new_password(*user_id, form.0.new_password, &pool)
        .await
        .map_err(e500)?;
    FlashMessage::error("Your password has been changed.").send();
    Ok(see_other("/admin/password"))
}
