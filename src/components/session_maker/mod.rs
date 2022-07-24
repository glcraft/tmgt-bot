use std::{collections::HashMap, borrow::Cow};

use cddio_core::ApplicationCommandEmbed;
use cddio_macros::component;
use futures_locks::RwLock;
use super::utils::data::Data;
use serde::{Serialize, Deserialize};
use serenity::{
    model::{
        id::{ChannelId, GuildId, UserId}, 
        event::{ReadyEvent, VoiceStateUpdateEvent, PresenceUpdateEvent}, 
        voice::VoiceState, gateway::Presence
    }, 
    client::Context
};


pub struct SessionMaker {
    data: RwLock<Data<DataSessions>>,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
struct ChannelSession(ChannelId);
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
struct ChannelSessionMaker(ChannelId);

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
struct ChannelSessions(Vec<ChannelSession>);

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
struct GuildData {
    sessions: ChannelSessions,
    session_maker: Option<ChannelSessionMaker>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
struct DataSessions {
    guilds: HashMap<GuildId, GuildData>,
}


#[component]
#[group(name="session", description="Gestion des sessions")]
impl SessionMaker {
    #[event(Ready)]
    async fn on_ready(&self, ctx: &Context, _: &ReadyEvent) {
        let guilds = self.data.read().await.read().guilds.iter().map(|(guild_id, _)| *guild_id).collect::<Vec<_>>();
        for guild_id in guilds {
            match self.check_sessions(ctx, guild_id).await {
                Ok(_) => {},
                Err(e) => println!("{}", e),
            }
        }
    }
    #[event(VoiceStateUpdate)]
    async fn on_channel_update(&self, ctx: &Context, voice_update: &VoiceStateUpdateEvent) {
        match voice_update.voice_state.channel_id {
            Some(_) => self.on_voice_connect(ctx, &voice_update.voice_state).await,
            None => self.on_voice_disconnect(ctx, &voice_update.voice_state).await,
        }
    }
    #[event(PresenceUpdate)]
    async fn on_presence_update(&self, ctx: &Context, presence_update: &PresenceUpdateEvent) {
        let guild_id = match presence_update.presence.guild_id {
            Some(guild_id) => guild_id,
            None => return,
        };
        let session_channel = match self.find_session_by_user(ctx, guild_id, presence_update.presence.user.id).await {
            Some(session) => session,
            None => return,
        };
        let session_name = match Self::get_session_name_from_presence(&presence_update.presence) {
            Some(name) => name,
            None => "Session",
        };
        
        match session_channel.edit(ctx, |m| m.name(session_name)).await {
            Ok(_) => {},
            Err(e) => println!("Erreur lors du renommange du salon vocal {}: {}", session_channel, e),
        }
    }
    #[command(group="session", name="set", description="Changer un salon en créateur de session")]
    async fn session_set(&self, ctx: &Context, app_cmd: ApplicationCommandEmbed<'_>, 
        #[argument(description="Salon vocal à changer")]
        salon_vocal: ChannelId
    ) {
        use serenity::model::channel::ChannelType;
        let guild_id = match app_cmd.0.guild_id {
            Some(v) => v,
            None => {
                println!("Cette commande ne peut être utilisée en message privé.");
                return;
            }
        };
        let channels = match guild_id.channels(ctx).await {
            Ok(v) => v,
            Err(e) => {
                println!("Impossible de récupérer les salons de ce serveur: {}", e.to_string());
                return;
            }
        };
        let found_channel = channels.into_iter().find_map(|(c_id, channel)| {
            if c_id == salon_vocal {
                Some(channel)
            } else {
                None
            }
        });
        let channel = match found_channel {
            Some(v) => v,
            None => {
                println!("Salon vocal introuvable.");
                return;
            }
        };
        if channel.kind != ChannelType::Voice {
            println!("Ce salon n'est pas un salon vocal.");
            return;
        }
        if let None = channel.parent_id{
            println!("Ce salon n'est pas dans une catégorie.");
            return;
        }
        {
            let mut data = self.data.write().await;
            let mut data = data.write();
            let guild_data = data.guilds.entry(guild_id).or_default();
            guild_data.session_maker = Some(ChannelSessionMaker(salon_vocal));
        }
    }
}
impl SessionMaker {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(Data::from_file("sessions.json").unwrap_or_default()),
        }
    }
    async fn on_voice_connect(&self, ctx: &Context, voice_state: &VoiceState) {
        let guild_id = match voice_state.guild_id {
            Some(guild_id) => guild_id,
            None => return,
        };
        let voice_channel_id = voice_state.channel_id.unwrap();
        let current_session_maker = self.data
            .read().await
            .read()
            .guilds
            .get(&guild_id)
            .and_then(|guild_data| guild_data.session_maker.clone());
        match current_session_maker {
            Some(current_session_maker) if current_session_maker.0 == voice_channel_id => (),
            _ => return,
        }
        match self.create_session(ctx, guild_id, voice_state.user_id).await {
            Ok(_) => (),
            Err(e) => println!("Erreur lors de la création de la session: {}", e),
        }
    }
    async fn on_voice_disconnect(&self, ctx: &Context, voice_state: &VoiceState) {
        let guild_id = match voice_state.guild_id {
            Some(guild_id) => guild_id,
            None => return,
        };
        match self.check_sessions(ctx, guild_id).await {
            Ok(_) => (),
            Err(e) => println!("Erreur lors de la suppression de la session: {}", e),
        }
    }
    async fn create_session(&self, ctx: &Context, guild_id: GuildId, user_id: UserId) -> Result<(), Cow<'static, str>> {
        let session_maker_id = match self.data.read().await.read().guilds.iter().find_map(|(_, guild_data)| {
            guild_data.session_maker.as_ref().map(|session_maker| {
                session_maker.0
            })
        }) {
            Some(session_maker_id) => session_maker_id,
            None => return Err(Cow::Borrowed("Aucun salon de création de session n'est défini")),
        };
        let channels = match guild_id.channels(ctx).await {//.into_iter().find(|(channel_id, _)| c.)
            Ok(channels) => channels,
            Err(e) => return Err(Cow::Owned(format!("Impossible de récupérer les salons du serveur {}: {}", guild_id.0, e.to_string()))),
        };
        let session_maker = match channels.into_iter()
            .find_map(|(channel_id, guild_channel)| {
                if channel_id == session_maker_id {
                    Some(guild_channel)
                } else {
                    None
                }
            }) {
            Some(session_maker) => session_maker,
            None => return Err(Cow::Borrowed("Aucun salon de création de session n'est défini")),
        };
        let parent_id = match session_maker.parent_id {
            Some(parent_id) => parent_id,
            None => return Err(Cow::Borrowed("Le salon de création de session n'a pas de parent")),
        };
        let session_name = match Self::get_session_name_from_user(ctx, guild_id, user_id).await {
            Some(name) => name,
            None => "Session".to_string(),
        };
        let session = match guild_id.create_channel(ctx, |create_channel| {
            create_channel.name(session_name)
                .kind(serenity::model::channel::ChannelType::Voice)
                .category(parent_id)
        }).await {
            Ok(session_id) => session_id,
            Err(e) => return Err(Cow::Owned(format!("Impossible de créer le salon de session: {}", e.to_string()))),
        };
        {
            let mut data = self.data.write().await;
            let mut data = data.write();
            let guild_data = data.guilds.entry(guild_id).or_default();
            guild_data.sessions.0.push(ChannelSession(session.id));
        }
        let member = match guild_id.member(ctx, user_id).await {
            Ok(member) => member,
            Err(e) => return Err(Cow::Owned(format!("Impossible de récupérer le membre du serveur: {}", e.to_string()))),
        };
        match member.move_to_voice_channel(ctx, session).await {
            Err(e) => return Err(Cow::Owned(format!("Impossible de déplacer le membre du serveur: {}", e.to_string()))),
            Ok(_) => (),
        };
        Ok(())
    }
    async fn check_sessions(&self, ctx: &Context, guild_id: GuildId) -> Result<(), Cow<'static, str>> {
        let sessions = {
            let data = self.data.read().await;
            let data = data.read();
            let guild_data = data.guilds.get(&guild_id).unwrap();
            guild_data.sessions.0.clone()
        };
        // let mut sessions = sessions.into_iter();
        let guild_channels = match guild_id.channels(ctx).await {
            Ok(channels) => channels,
            Err(e) => return Err(Cow::Owned(format!("Impossible de récupérer les salons du serveur {}: {}", guild_id.0, e.to_string()))),
        };
        let sessions_channel = guild_channels.into_iter().filter(|(c_id, _)| sessions.contains(&ChannelSession(*c_id))).collect::<Vec<_>>();
        for (session_channel_id, session_channel) in sessions_channel {
            let members = match session_channel.members(ctx).await {
                Ok(members) => members,
                Err(e) => {
                    println!("Erreur lors de la récupération des membres du salon de session: {}", e.to_string());
                    continue;
                },
            };
            if members.len() == 0 {
                match self.delete_session(ctx, guild_id, session_channel_id).await {
                    Ok(_) => (),
                    Err(e) => println!("Erreur lors de la suppression du salon de session: {}", e.to_string()),
                }
            }
        }
        Ok(())
    }
    async fn delete_session(&self, ctx: &Context, guild_id: GuildId, session_id: ChannelId) -> Result<(), Cow<'static, str>> {
        match session_id.delete(ctx).await {
            Ok(_) => (),
            Err(e) => return Err(Cow::Owned(format!("Impossible de supprimer le salon de session: {}", e.to_string()))),
        };
        {
            let mut data = self.data.write().await;
            let mut data = data.write();
            let guild_data = data.guilds.get_mut(&guild_id).unwrap();
            guild_data.sessions.0.retain(|channel_session| channel_session.0 != session_id);
        }
        Ok(())
    }
    async fn find_session_by_user(&self, ctx: &Context, guild_id: GuildId, user_id: UserId) -> Option<ChannelId> {
        let sessions = {
            let data = self.data.read().await;
            let data = data.read();
            let guild_data = data.guilds.get(&guild_id).unwrap();
            guild_data.sessions.0.clone()
        };
        let guild_channels = match guild_id.channels(ctx).await {
            Ok(channels) => channels,
            _ => return None,
        };
        let sessions_channel = guild_channels.into_iter().filter(|(c_id, _)| sessions.contains(&ChannelSession(*c_id))).collect::<Vec<_>>();
        for (session_channel_id, session_channel) in sessions_channel {
            let members = match session_channel.members(ctx).await {
                Ok(members) => members,
                Err(e) => {
                    println!("Erreur lors de la récupération des membres du salon de session: {}", e.to_string());
                    continue;
                },
            };
            if members.iter().any(|member| member.user.id == user_id) {
                return Some(session_channel_id);
            }
        }
        None
    }
    async fn get_session_name_from_user(ctx: &Context, guild_id: GuildId, user_id: UserId) -> Option<String> {
        let guild = guild_id.to_guild_cached(ctx)?;
        println!("presences:\n{:?}", guild.presences);
        let presence = guild.presences.iter().find_map(|(presence_user_id, presence)| {
            if *presence_user_id == user_id {
                Some(presence)
            } else {
                None
            }
        })?;
        Self::get_session_name_from_presence(presence).map(String::from)
    }
    fn get_session_name_from_presence<'a>(presence: &'a Presence) -> Option<&'a str> {
        Some(presence.activities.first()?.name.as_str())
    }
}