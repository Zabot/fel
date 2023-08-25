use std::env;

use anyhow::Result;
use git2::{Cred, RemoteCallbacks};

pub fn callbacks() -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::default();
    callbacks.credentials(|_url, username_from_url, _allowed_types| {
        Cred::ssh_key(
            username_from_url.unwrap(),
            None,
            std::path::Path::new(&format!("{}/.ssh/id_rsa", env::var("HOME").unwrap())),
            None,
        )
    });

    callbacks
}
