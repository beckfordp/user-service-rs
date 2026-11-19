// SPDX-License-Identifier: MIT

//! The recipient lookup (ADR 0025): gluon-rs `user-client`'s `UserClient`,
//! the one notification-service-rs's consumers share (gluon TD-59).

use user_client::{LookupError, UserClient};

use crate::handler::Lookup;

impl Lookup for UserClient {
    async fn email(&self, user_id: &str) -> Result<Option<String>, LookupError> {
        UserClient::email(self, user_id).await
    }
}
