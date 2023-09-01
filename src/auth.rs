use std::env;

use git2::{Cred, RemoteCallbacks};

pub fn callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::default();
    callbacks.credentials(|url, username_from_url, allowed_types| {
        tracing::trace!(
            ?url,
            ?username_from_url,
            ?allowed_types,
            "providing auth credentials"
        );
        Cred::ssh_key(
            username_from_url.unwrap(),
            None,
            std::path::Path::new(&format!("{}/.ssh/id_rsa", env::var("HOME").unwrap())),
            None,
        )
    });

    callbacks
}
