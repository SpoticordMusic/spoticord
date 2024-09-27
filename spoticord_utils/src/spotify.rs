use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use librespot::{
    core::{Session, SessionConfig},
    discovery::Credentials,
    protocol::authentication::AuthenticationType,
};
use log::debug;
use std::time::Duration;

pub async fn validate_token(
    username: impl Into<String>,
    token: impl Into<String>,
) -> Result<Option<String>> {
    let auth_data = BASE64.decode(token.into())?;

    let credentials = Credentials {
        username: Some(username.into()),
        auth_type: AuthenticationType::AUTHENTICATION_STORED_SPOTIFY_CREDENTIALS,
        auth_data,
    };

    debug!("Validating session token for {:?}", credentials.username);

    let new_credentials = request_session_token(credentials.clone()).await?;

    if credentials.auth_data != new_credentials.auth_data {
        debug!("New session token retrieved for {:?}", credentials.username);

        return Ok(Some(BASE64.encode(new_credentials.auth_data)));
    }

    Ok(None)
}

pub async fn request_session_token(credentials: Credentials) -> Result<Credentials> {
    debug!("Requesting session token for {:?}", credentials.username);

    let session = Session::new(SessionConfig::default(), None);
    let mut tries = 0;

    Ok(loop {
        match connect(&session, credentials.clone()).await {
            Ok(creds) => break creds,
            Err(e) => {
                tries += 1;
                if tries > 3 {
                    return Err(e);
                }

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    })
}

/// Wrapper around session connecting that times out if an operation is still busy after 3 seconds
async fn connect(session: &Session, credentials: Credentials) -> Result<Credentials> {
    const TIMEOUT: Duration = Duration::from_secs(3);

    let (host, port) =
        tokio::time::timeout(TIMEOUT, session.apresolver().resolve("accesspoint")).await??;

    // `connect` already has a 3 second timeout internally
    let mut transport = librespot::core::connection::connect(&host, port, None).await?;

    let creds = tokio::time::timeout(
        TIMEOUT,
        librespot::core::connection::authenticate(
            &mut transport,
            credentials.clone(),
            &session.config().device_id,
        ),
    )
    .await??;

    Ok(creds)
}
