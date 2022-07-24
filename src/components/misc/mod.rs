use cddio_macros::component;
use serenity::{client::Context, model::{event::ReadyEvent, Permissions}};


pub struct Misc {
    permissions: u64
}

#[component]
impl Misc {
    #[event(Ready)]
    async fn on_ready(&self, ctx: &Context, ready: &ReadyEvent) {
        let perms = match Permissions::from_bits(self.permissions) {
            Some(perms) => perms,
            None => {
                println!("Permissions invalides");
                return;
            }
        };
        match ready.ready.user.invite_url(&ctx.http, perms).await {
            Ok(v) => println!("Invitation: {}", v),
            Err(e) => println!("Lien d'invitation impossible à créer: {}", e.to_string()),
        }
    }
}

impl Misc {
    pub fn new(permissions: u64) -> Self {
        Self {
            permissions
        }
    }
}