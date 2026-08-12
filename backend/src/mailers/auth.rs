//! Transactional e-mail for the account-recovery flow.
//!
//! The only mailer in the application, and the only reason the `mailer:` block
//! in `config/*.yaml` exists: `POST /api/auth/forgot` has no other way to prove
//! the caller owns the address it was given.
//!
//! The reset link points at the SPA route (`/reset?token=…`), not at an API
//! endpoint — the token is consumed by `POST /api/auth/reset`, which the screen
//! calls once the user has typed a new password. Sending the user straight to
//! the API would hand the token to whatever renders the response.

use loco_rs::prelude::*;

use crate::models::_entities::users;

static FORGOT: Dir<'_> = include_dir!("src/mailers/auth/forgot");

/// Sender of every account e-mail.
pub struct AuthMailer {}

#[async_trait]
impl Mailer for AuthMailer {
    fn opts() -> mailer::MailerOpts {
        mailer::MailerOpts {
            from: "DB Backup Manager <no-reply@backup-manager.local>".to_string(),
            ..Default::default()
        }
    }
}

impl AuthMailer {
    /// Sends the password-reset link for `user`.
    ///
    /// Requires `reset_token` to be already set on the model — the caller owns
    /// the token lifecycle, so that a resend does not silently issue a second
    /// token and invalidate the first link the user clicked.
    ///
    /// # Errors
    ///
    /// Fails when the template cannot be rendered or the message cannot be
    /// enqueued for delivery.
    pub async fn forgot_password(ctx: &AppContext, user: &users::Model) -> Result<()> {
        let Some(token) = user.reset_token.as_ref() else {
            return Err(Error::Message(
                "cannot send a reset e-mail without a reset token".to_string(),
            ));
        };

        Self::mail_template(
            ctx,
            &FORGOT,
            mailer::Args {
                to: user.email.clone(),
                locals: data!({
                    "name": user.full_name.clone().unwrap_or_else(|| user.email.clone()),
                    "resetUrl": reset_url(ctx, token),
                }),
                ..Default::default()
            },
        )
        .await
    }
}

/// Absolute URL of the SPA screen that finishes the reset.
fn reset_url(ctx: &AppContext, token: &str) -> String {
    format!("{}/reset?token={token}", ctx.config.server.full_url())
}
