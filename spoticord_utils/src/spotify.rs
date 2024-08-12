use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use librespot::{
    core::{connection::AuthenticationError, Session, SessionConfig},
    discovery::Credentials,
    protocol::{authentication::AuthenticationType, keyexchange::ErrorCode},
};
use log::trace;

pub async fn validate_token(
    username: impl Into<String>,
    token: impl Into<String>,
) -> Result<Option<String>> {
    let auth_data = BASE64.decode(token.into())?;

    let credentials = Credentials {
        username: username.into(),
        auth_type: AuthenticationType::AUTHENTICATION_STORED_SPOTIFY_CREDENTIALS,
        auth_data,
    };

    trace!("Validating session token for {}", credentials.username);

    let new_credentials = request_session_token(credentials.clone()).await?;

    if credentials.auth_data != new_credentials.auth_data {
        trace!("New session token retrieved for {}", credentials.username);

        return Ok(Some(BASE64.encode(new_credentials.auth_data)));
    }

    Ok(None)
}

pub async fn request_session_token(credentials: Credentials) -> Result<Credentials> {
    trace!("Requesting session token for {}", credentials.username);

    let session = Session::new(SessionConfig::default(), None);
    let mut tries = 0;

    Ok(loop {
        let (host, port) = session.apresolver().resolve("accesspoint").await?;
        let mut transport = librespot::core::connection::connect(&host, port, None).await?;

        match librespot::core::connection::authenticate(
            &mut transport,
            credentials.clone(),
            &session.config().device_id,
        )
        .await
        {
            Ok(creds) => break creds,
            Err(e) => {
                if let Some(AuthenticationError::LoginFailed(ErrorCode::TryAnotherAP)) =
                    e.error.downcast_ref::<AuthenticationError>()
                {
                    tries += 1;
                    if tries > 3 {
                        return Err(e.into());
                    }

                    continue;
                } else {
                    return Err(e.into());
                }
            }
        };
    })
}
