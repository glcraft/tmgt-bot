//! Core de l'application. 
//! L'initialisation du bot et la gestion des composants se fait dans ce module.

use futures_locks::RwLock;
use serenity::{Client, model::id::{ApplicationId, UserId}, prelude::GatewayIntents};
use crate::{components as cmp, config::Config};
use cddio_core as core;

type Result<T> = serenity::Result<T>;

/// Structure du bot.
/// 
/// Il s'agit de la classe mère de l'application. 
/// 
/// Le bot est composé de plusieurs composants qui sont créés et placés dans un [ComponentContainer]. 
/// Le conteneur retourne au client du bot un [ComponentEventDispatcher], un event dispatcher 
/// qui se charge de déployer les événements Discord dans les composants.
/// 
/// [ComponentContainer]: core::ComponentContainer
/// [ComponentEventDispatcher]: core::event::ComponentEventDispatcher
pub struct Bot {
    /// Client discord de serenity
    client: Client,
    /// Handler des composants.
    /// Actuellement un vecteur mais prochainement un gestionnaire est prévu.
    _cmp_container: RwLock<core::ComponentContainer>
}

impl Bot {
    /// Crée un nouveau bot et l'initialise.
    pub async fn new(config: &Config) -> Result<Bot> {
        let owners_id = config.owners
            .iter()
            .map(|id| id.parse::<u64>().unwrap())
            .map(|id| UserId(id))
            .collect::<Vec<_>>();
        let app_id = ApplicationId(config.app_id);
        let perms = config.permissions;
        let ref_container = RwLock::new(core::ComponentContainer::new());
        {
            let mut container = ref_container.write().await;
            container.add_component(cmp::Help::new(ref_container.clone()));
            container.add_component(cmp::SlashCommand::new(app_id, ref_container.clone(), owners_id));
            container.add_component(cmp::SessionMaker::new());
            container.add_component(cmp::Misc::new(perms));
        }
        let client = Client::builder(&config.token, GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_PRESENCES)
            .raw_event_handler(ref_container.read().await.get_event_dispatcher())
            .application_id(config.app_id)
            .await?;
        Ok(Bot{
            client,
            _cmp_container: ref_container
        })
    }
    /// Lance le bot.
    pub async fn start(&mut self) -> Result<()> {
        self.client.start().await
    }
}

//TODO: Enregistrer la configuration du bot lors du drop de ce dernier
impl Drop for Bot {
    fn drop(&mut self) {
        println!("Bot dropped");
    }
}