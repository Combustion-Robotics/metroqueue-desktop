//! A Discord bot that mirrors this app's playback into voice channels. Off by default.
//!
//! The user registers a bot application of their own in the Discord developer portal and pastes
//! its token into settings. It is stored as `discord_bot_token`, next to the other credentials and
//! like them never in `UI_SETTINGS`, so it never crosses into the webview. Server members only bring
//! the bot in and out (`/join`, `/leave`) and ask what is playing (`/np`). Playback itself is
//! controlled from this app alone.
//!
//! The desktop side publishes its playback into a `watch` channel ([`Desktop`]) from the same choke
//! points that feed rich presence (`state.rs`). The bot task runs only while the bot is enabled and
//! has a token. It turns each change into songbird actions ([`Mirror::step`]) for every guild's
//! call, all playing the same stream. A track is streamed again from what mpv was handed
//! ([`remember_source`]): the loopback audio proxy, the googlevideo URL itself when the user has a
//! proxy, or the local file.
//!
//! Voice needs songbird 0.6 or newer: it is the first release that speaks DAVE, the end-to-end
//! encryption Discord now requires on every voice connection.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use innertube::SongItem;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serenity::all::{
    Command, CommandInteraction, Context, CreateCommand, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, EditInteractionResponse, EventHandler, GatewayError,
    GatewayIntents, GuildId, Interaction, Permissions, Ready, VoiceState,
};
use songbird::input::{File, HttpRequest, Input};
use songbird::tracks::{Track, TrackHandle};
use songbird::{Call, SerenityInit, Songbird};
use tauri::Emitter;
use tokio::sync::{mpsc, oneshot, watch};

use crate::db::Db;

const TOKEN_KEY: &str = "discord_bot_token";
const ENABLED_KEY: &str = "discord_bot_enabled";
const STATUS_EVENT: &str = "discord-bot://status";
/// How far the bot may drift from the desktop before it seeks. Position ticks arrive about once a
/// second, so anything tighter would chase tick jitter.
const DRIFT_SECS: f64 = 2.0;
/// Sources kept for [`source_for`]: the playing track and the gapless one after it, with room.
const MAX_SOURCES: usize = 8;

/// What the settings tab shows, and what `discord-bot://status` carries.
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordBotStatus {
    enabled: bool,
    has_token: bool,
    state: BotState,
    error: Option<String>,
    bot_name: Option<String>,
    invite_url: Option<String>,
    voice: Vec<VoiceChannel>,
}

#[derive(Clone, Copy, Default, PartialEq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum BotState {
    #[default]
    Off,
    Connecting,
    Online,
    Error,
}

#[derive(Clone, serde::Serialize)]
struct VoiceChannel {
    guild: String,
    channel: String,
}

/// Where a track's audio can be fetched again.
#[derive(Clone, Debug, PartialEq)]
enum Source {
    Http { url: String, headers: HeaderMap },
    File(PathBuf),
}

static SOURCES: Mutex<Vec<(String, Source)>> = Mutex::new(Vec::new());

/// Remember the URL (or path) mpv was handed for `video_id`, so the bot can stream the same thing
/// once that track starts. Called for the gapless lookahead too, which is why this is keyed.
pub fn remember_source(video_id: &str, url: &str, headers: &HashMap<String, String>) {
    let source = if url.starts_with("http://") || url.starts_with("https://") {
        let headers = headers
            .iter()
            .filter_map(|(k, v)| {
                Some((HeaderName::try_from(k.as_str()).ok()?, HeaderValue::from_str(v).ok()?))
            })
            .collect();
        Source::Http { url: url.to_owned(), headers }
    } else if url.contains("://") {
        return;
    } else {
        Source::File(url.into())
    };
    let Ok(mut sources) = SOURCES.lock() else { return };
    sources.retain(|(id, _)| id != video_id);
    if sources.len() >= MAX_SOURCES {
        sources.remove(0);
    }
    sources.push((video_id.to_owned(), source));
}

fn source_for(video_id: &str) -> Option<Source> {
    let sources = SOURCES.lock().ok()?;
    sources.iter().rev().find(|(id, _)| id == video_id).map(|(_, s)| s.clone())
}

/// A position that runs on its own while playing: the last known one plus the time since.
#[derive(Clone, Copy, Debug)]
struct Clock {
    playing: bool,
    position: f64,
    at: Instant,
}

impl Clock {
    fn position_at(&self, now: Instant) -> f64 {
        if self.playing {
            self.position + now.saturating_duration_since(self.at).as_secs_f64()
        } else {
            self.position
        }
    }
}

#[derive(Debug)]
struct NowPlaying {
    video_id: String,
    title: String,
    artists: String,
    source: Option<Source>,
}

/// The desktop's playback as the bot follows it.
#[derive(Clone, Debug)]
struct Desktop {
    track: Option<Arc<NowPlaying>>,
    /// Bumped on every track start, including a repeat-one loop of the same track.
    seq: u64,
    clock: Clock,
}

#[derive(Debug, PartialEq)]
enum Action {
    Load { at: f64, playing: bool },
    Stop,
    Play,
    Pause,
    Seek(f64),
}

/// The bot's side: which track start it last loaded and where it believes its calls are.
struct Mirror {
    seq: u64,
    clock: Clock,
}

impl Mirror {
    fn new(now: Instant) -> Self {
        Mirror { seq: 0, clock: Clock { playing: false, position: 0.0, at: now } }
    }

    /// What the calls have to do to match `d`.
    fn step(&mut self, d: &Desktop, now: Instant) -> Vec<Action> {
        let want = d.clock.position_at(now);
        if d.seq != self.seq {
            self.seq = d.seq;
            self.clock = Clock { playing: d.clock.playing, position: want, at: now };
            let playable = d.track.as_ref().is_some_and(|t| t.source.is_some());
            return vec![if playable {
                Action::Load { at: want, playing: d.clock.playing }
            } else {
                Action::Stop
            }];
        }
        let mut out = Vec::new();
        if d.clock.playing != self.clock.playing {
            let position = self.clock.position_at(now);
            self.clock = Clock { playing: d.clock.playing, position, at: now };
            out.push(if d.clock.playing { Action::Play } else { Action::Pause });
        }
        // shortcut: the bot plays at 1x, without crossfades, tempo, normalization or other effects.
        // A tempo change shows up here as drift and is answered with a seek.
        if (self.clock.position_at(now) - want).abs() > DRIFT_SECS {
            self.clock.position = want;
            self.clock.at = now;
            out.push(Action::Seek(want));
        }
        out
    }
}

/// Status plus the run generation it belongs to, so a bot that is shutting down cannot overwrite
/// the status of the one that replaced it.
struct Shared {
    app: tauri::AppHandle,
    status: Mutex<DiscordBotStatus>,
    gen: AtomicU64,
}

impl Shared {
    fn update(&self, gen: u64, f: impl FnOnce(&mut DiscordBotStatus)) {
        let snapshot = {
            let mut status = self.status.lock().unwrap();
            if self.gen.load(Ordering::SeqCst) != gen {
                return;
            }
            f(&mut status);
            status.clone()
        };
        let _ = self.app.emit(STATUS_EVENT, snapshot);
    }
}

pub struct DiscordBot {
    shared: Arc<Shared>,
    desktop: watch::Sender<Desktop>,
    task: Mutex<Option<(oneshot::Sender<()>, tauri::async_runtime::JoinHandle<()>)>>,
}

impl DiscordBot {
    pub fn new(app: tauri::AppHandle) -> Self {
        let now = Instant::now();
        let desktop = Desktop {
            track: None,
            seq: 0,
            clock: Clock { playing: false, position: 0.0, at: now },
        };
        DiscordBot {
            shared: Arc::new(Shared { app, status: Mutex::default(), gen: AtomicU64::new(0) }),
            desktop: watch::Sender::new(desktop),
            task: Mutex::new(None),
        }
    }

    pub fn status(&self) -> DiscordBotStatus {
        self.shared.status.lock().unwrap().clone()
    }

    /// Start, restart or stop the bot to match the stored settings.
    pub fn apply(&self, db: &Db) {
        let enabled = db.get_setting(ENABLED_KEY).as_deref() == Some("true");
        let token = db.get_setting(TOKEN_KEY).filter(|t| !t.is_empty());
        let start = enabled && token.is_some();
        let mut task = self.task.lock().unwrap();
        let gen = {
            let mut status = self.shared.status.lock().unwrap();
            let gen = self.shared.gen.fetch_add(1, Ordering::SeqCst) + 1;
            *status = DiscordBotStatus {
                enabled,
                has_token: token.is_some(),
                state: if start { BotState::Connecting } else { BotState::Off },
                ..Default::default()
            };
            let _ = self.shared.app.emit(STATUS_EVENT, status.clone());
            gen
        };
        let old = task.take().map(|(stop, handle)| {
            let _ = stop.send(());
            handle
        });
        let Some(token) = token.filter(|_| start) else { return };
        let (stop_tx, stop_rx) = oneshot::channel();
        let (shared, desktop) = (self.shared.clone(), self.desktop.subscribe());
        let handle = tauri::async_runtime::spawn(async move {
            // One gateway session per token at a time: let the old bot leave its calls first.
            if let Some(old) = old {
                let _ = old.await;
            }
            if let Err(e) = run(shared.clone(), gen, &token, desktop, stop_rx).await {
                tracing::warn!(error = %e, "discord bot stopped");
                let message = error_message(&e);
                shared.update(gen, |s| {
                    *s = DiscordBotStatus {
                        enabled: s.enabled,
                        has_token: s.has_token,
                        state: BotState::Error,
                        error: Some(message),
                        ..Default::default()
                    }
                });
            }
        });
        *task = Some((stop_tx, handle));
    }

    /// Store `token` (or clear it, when empty) and restart the bot to match.
    pub fn set_token(&self, db: &Db, token: &str) -> Result<(), String> {
        let token = normalize_token(token)?;
        db.set_setting(TOKEN_KEY, &token);
        self.apply(db);
        Ok(())
    }

    pub fn set_enabled(&self, db: &Db, enabled: bool) {
        db.set_setting(ENABLED_KEY, if enabled { "true" } else { "false" });
        self.apply(db);
    }

    /// A track started from the top (the app never starts one anywhere else; a later position
    /// tick corrects a restored or Listen Together start).
    pub fn set_track(&self, item: &SongItem) {
        let track = Arc::new(NowPlaying {
            video_id: item.video_id.clone(),
            title: item.title.clone(),
            artists: item.artists.clone(),
            source: source_for(&item.video_id),
        });
        self.desktop.send_modify(|d| {
            d.track = Some(track);
            d.seq += 1;
            d.clock.position = 0.0;
            d.clock.at = Instant::now();
        });
    }

    pub fn set_playing(&self, playing: bool) {
        self.desktop.send_if_modified(|d| {
            if d.clock.playing == playing {
                return false;
            }
            let now = Instant::now();
            d.clock = Clock { playing, position: d.clock.position_at(now), at: now };
            true
        });
    }

    pub fn set_position(&self, secs: f64) {
        self.desktop.send_modify(|d| {
            d.clock.position = secs;
            d.clock.at = Instant::now();
        });
    }
}

/// The token as stored: trimmed, without a pasted `Bot ` prefix, and empty to clear it.
fn normalize_token(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    let token = token.strip_prefix("Bot ").unwrap_or(token).trim();
    if token.is_empty() || token_user_id(token).is_some() {
        Ok(token.to_owned())
    } else {
        Err("That is not a bot token. Copy it from the Bot page of your application in the \
             Discord developer portal."
            .into())
    }
}

/// The bot's user id, which a Discord token carries base64-encoded in the first of its three
/// dot-separated parts. `None` for anything that is not shaped like a token.
fn token_user_id(token: &str) -> Option<u64> {
    let b64 = |s: &str| {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    };
    let mut parts = token.split('.');
    let (id, stamp, mac) = (parts.next()?, parts.next()?, parts.next()?);
    let id = id.trim_end_matches('=');
    if parts.next().is_some() || !b64(id) || !b64(stamp) || !b64(mac) {
        return None;
    }
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(id).ok()?;
    std::str::from_utf8(&raw).ok()?.parse().ok().filter(|&id| id > 0)
}

fn invite_permissions() -> Permissions {
    Permissions::VIEW_CHANNEL
        | Permissions::SEND_MESSAGES
        | Permissions::CONNECT
        | Permissions::SPEAK
}

fn invite_url(application_id: u64) -> String {
    format!(
        "https://discord.com/oauth2/authorize?client_id={application_id}\
         &scope=bot%20applications.commands&permissions={}",
        invite_permissions().bits()
    )
}

/// Serenity reconnects on its own after network trouble, so what ends a session is either one of
/// these or something it could not recover from.
fn error_message(e: &serenity::Error) -> String {
    match e {
        serenity::Error::Gateway(GatewayError::InvalidAuthentication) => {
            "Discord rejected the bot token. Reset it in the developer portal and paste the new one."
                .into()
        }
        serenity::Error::Gateway(
            GatewayError::InvalidGatewayIntents | GatewayError::DisallowedGatewayIntents,
        ) => "Discord refused the bot's gateway intents.".into(),
        _ => "Lost the connection to Discord. Turn the bot off and on to retry.".into(),
    }
}

fn track_link(video_id: &str) -> Option<String> {
    (!crate::local::is_local_song(video_id))
        .then(|| format!("https://music.youtube.com/watch?v={video_id}"))
}

/// One gateway session, until it is told to stop or fails.
async fn run(
    shared: Arc<Shared>,
    gen: u64,
    token: &str,
    desktop: watch::Receiver<Desktop>,
    stop: oneshot::Receiver<()>,
) -> serenity::Result<()> {
    let songbird = Songbird::serenity();
    let (joined_tx, joined_rx) = mpsc::unbounded_channel();
    let handler = Handler {
        gen,
        shared,
        songbird: songbird.clone(),
        desktop: desktop.clone(),
        joined: joined_tx,
        commands_set: AtomicBool::new(false),
    };
    let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES;
    let mut client = serenity::Client::builder(token, intents)
        .event_handler(handler)
        .register_songbird_with(songbird.clone())
        .await?;
    let shards = client.shard_manager.clone();
    let result = tokio::select! {
        // A stop that is already waiting wins, so a superseded bot never connects at all.
        biased;
        _ = stop => Ok(()),
        r = client.start() => r.and(Err(serenity::Error::Other("Discord closed the connection"))),
        _ = mirror(&songbird, desktop, joined_rx) => Ok(()),
    };
    let guilds: Vec<GuildId> = songbird.iter().map(|(g, _)| g.0.into()).collect();
    for guild in guilds {
        let _ = songbird.remove(guild).await;
    }
    shards.shutdown_all().await;
    result
}

/// Follow the desktop for as long as it is there, in every call the bot is in.
async fn mirror(
    songbird: &Songbird,
    mut desktop: watch::Receiver<Desktop>,
    mut joined: mpsc::UnboundedReceiver<GuildId>,
) {
    let mut mirror = Mirror::new(Instant::now());
    let mut handles: HashMap<GuildId, TrackHandle> = HashMap::new();
    loop {
        let d = desktop.borrow_and_update().clone();
        for action in mirror.step(&d, Instant::now()) {
            match action {
                Action::Load { at, playing } => {
                    handles.clear();
                    let calls: Vec<_> = songbird.iter().collect();
                    for (guild, call) in calls {
                        if let Some(h) = load(&call, &d, at, playing).await {
                            handles.insert(guild.0.into(), h);
                        }
                    }
                }
                Action::Stop => {
                    handles.clear();
                    let calls: Vec<_> = songbird.iter().collect();
                    for (_, call) in calls {
                        call.lock().await.stop();
                    }
                }
                Action::Play => handles.values().for_each(|h| drop(h.play())),
                Action::Pause => handles.values().for_each(|h| drop(h.pause())),
                Action::Seek(at) => {
                    handles.values().for_each(|h| drop(h.seek(Duration::from_secs_f64(at))))
                }
            }
        }
        tokio::select! {
            changed = desktop.changed() => if changed.is_err() { return },
            Some(guild) = joined.recv() => {
                let Some(call) = songbird.get(guild) else { continue };
                let d = desktop.borrow().clone();
                let now = Instant::now();
                if let Some(h) = load(&call, &d, mirror.clock.position_at(now), mirror.clock.playing).await {
                    handles.insert(guild, h);
                }
            }
        }
    }
}

/// Start the desktop's track in one call, at `at` seconds.
async fn load(
    call: &tokio::sync::Mutex<Call>,
    d: &Desktop,
    at: f64,
    playing: bool,
) -> Option<TrackHandle> {
    let input: Input = match d.track.as_ref()?.source.clone()? {
        Source::Http { url, headers } => {
            HttpRequest::new_with_headers(crate::http::client().clone(), url, headers).into()
        }
        Source::File(path) => File::new(path).into(),
    };
    let track = Track::new(input);
    let handle = call.lock().await.play_only(if playing { track } else { track.pause() });
    if at > 0.5 {
        let _ = handle.seek(Duration::from_secs_f64(at));
    }
    Some(handle)
}

fn commands() -> Vec<CreateCommand> {
    vec![
        CreateCommand::new("join")
            .description("Join your voice channel and play what MetroQueue Desktop is playing")
            .dm_permission(false),
        CreateCommand::new("leave").description("Leave the voice channel").dm_permission(false),
        CreateCommand::new("np").description("Show the track that is playing"),
    ]
}

struct Handler {
    gen: u64,
    shared: Arc<Shared>,
    songbird: Arc<Songbird>,
    desktop: watch::Receiver<Desktop>,
    joined: mpsc::UnboundedSender<GuildId>,
    commands_set: AtomicBool,
}

impl Handler {
    async fn reply(
        &self,
        ctx: &Context,
        cmd: &CommandInteraction,
        text: &str,
    ) -> serenity::Result<()> {
        let msg = CreateInteractionResponseMessage::new().content(text).ephemeral(true);
        cmd.create_response(&ctx.http, CreateInteractionResponse::Message(msg)).await
    }

    async fn join(&self, ctx: &Context, cmd: &CommandInteraction) -> serenity::Result<()> {
        let Some(guild) = cmd.guild_id else {
            return self.reply(ctx, cmd, "Use /join in a server.").await;
        };
        let channel = ctx
            .cache
            .guild(guild)
            .and_then(|g| g.voice_states.get(&cmd.user.id).and_then(|v| v.channel_id));
        let Some(channel) = channel else {
            return self.reply(ctx, cmd, "Join a voice channel first, then use /join.").await;
        };
        // Connecting (DAVE handshake included) can outlast the three seconds a reply is due in.
        cmd.defer_ephemeral(&ctx.http).await?;
        let text = match self.songbird.join(guild, channel).await {
            Ok(call) => {
                let _ = call.lock().await.deafen(true).await;
                let _ = self.joined.send(guild);
                format!("Joined <#{channel}>. I play whatever MetroQueue Desktop is playing.")
            }
            Err(e) => {
                tracing::info!(error = %e, "discord bot: joining a voice channel failed");
                let _ = self.songbird.remove(guild).await;
                format!("Couldn't join <#{channel}>. Check that I may connect and speak there.")
            }
        };
        cmd.edit_response(&ctx.http, EditInteractionResponse::new().content(text)).await.map(drop)
    }

    async fn leave(&self, ctx: &Context, cmd: &CommandInteraction) -> serenity::Result<()> {
        let left = match cmd.guild_id {
            Some(guild) if self.songbird.get(guild).is_some() => {
                self.songbird.remove(guild).await.is_ok()
            }
            _ => false,
        };
        let text =
            if left { "Left the voice channel." } else { "I'm not in a voice channel here." };
        self.reply(ctx, cmd, text).await
    }

    async fn now_playing(&self, ctx: &Context, cmd: &CommandInteraction) -> serenity::Result<()> {
        let (track, playing) = {
            let d = self.desktop.borrow();
            (d.track.clone(), d.clock.playing)
        };
        let Some(track) = track else {
            return self.reply(ctx, cmd, "Nothing is playing in MetroQueue Desktop.").await;
        };
        let mut embed = CreateEmbed::new().title(&track.title).description(if playing {
            track.artists.clone()
        } else {
            format!("{} (paused)", track.artists)
        });
        if let Some(link) = track_link(&track.video_id) {
            embed = embed.url(link);
        }
        let msg = CreateInteractionResponseMessage::new().embed(embed);
        cmd.create_response(&ctx.http, CreateInteractionResponse::Message(msg)).await
    }

    /// The voice channels the bot is in, by name, from the cache.
    fn voice(&self, ctx: &Context) -> Vec<VoiceChannel> {
        let me = ctx.cache.current_user().id;
        let guilds: Vec<GuildId> = self.songbird.iter().map(|(g, _)| g.0.into()).collect();
        let mut voice: Vec<VoiceChannel> = guilds
            .into_iter()
            .filter_map(|guild| {
                let g = ctx.cache.guild(guild)?;
                let channel = g.voice_states.get(&me)?.channel_id?;
                Some(VoiceChannel {
                    guild: g.name.clone(),
                    channel: g
                        .channels
                        .get(&channel)
                        .map_or(channel.to_string(), |c| c.name.clone()),
                })
            })
            .collect();
        voice.sort_by(|a, b| (&a.guild, &a.channel).cmp(&(&b.guild, &b.channel)));
        voice
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        if !self.commands_set.swap(true, Ordering::SeqCst) {
            if let Err(e) = Command::set_global_commands(&ctx.http, commands()).await {
                tracing::warn!(error = %e, "discord bot: registering slash commands failed");
                self.commands_set.store(false, Ordering::SeqCst);
            }
        }
        let invite = invite_url(ready.application.id.get());
        self.shared.update(self.gen, |s| {
            s.state = BotState::Online;
            s.error = None;
            s.bot_name = Some(ready.user.name.clone());
            s.invite_url = Some(invite);
        });
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Interaction::Command(cmd) = interaction else { return };
        let res = match cmd.data.name.as_str() {
            "join" => self.join(&ctx, &cmd).await,
            "leave" => self.leave(&ctx, &cmd).await,
            "np" => self.now_playing(&ctx, &cmd).await,
            _ => Ok(()),
        };
        if let Err(e) = res {
            tracing::debug!(error = %e, "discord bot: answering a command failed");
        }
    }

    async fn voice_state_update(&self, ctx: Context, _old: Option<VoiceState>, new: VoiceState) {
        if new.user_id != ctx.cache.current_user().id {
            return;
        }
        // Disconnected by someone else: drop the call, or it would linger without a channel.
        if let (None, Some(guild)) = (new.channel_id, new.guild_id) {
            let _ = self.songbird.remove(guild).await;
        }
        let voice = self.voice(&ctx);
        self.shared.update(self.gen, |s| s.voice = voice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Assembled from parts so secret scanners do not flag this fake token.
    const TOKEN: &str =
        concat!("MTIzNDU2Nzg5MDEyMzQ1Njc4", ".GaBcDe.", "abcdefghijklmnopqrstuvwxyz0123456789AB");

    #[test]
    fn tokens_are_checked_for_shape() {
        assert_eq!(token_user_id(TOKEN), Some(123456789012345678));
        assert_eq!(normalize_token(&format!("  Bot {TOKEN}\n")).unwrap(), TOKEN);
        assert_eq!(normalize_token("   ").unwrap(), "");
        for bad in ["hunter2", "a.b", "MTIz.x.y.z", "bm90IGRpZ2l0cw.abc.def", "MTIz.ab c.def"] {
            assert!(normalize_token(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn invite_asks_for_view_send_connect_speak() {
        assert_eq!(invite_permissions().bits(), 1024 + 2048 + 1048576 + 2097152);
        assert_eq!(
            invite_url(42),
            "https://discord.com/oauth2/authorize?client_id=42\
             &scope=bot%20applications.commands&permissions=3148800"
        );
    }

    fn desktop(seq: u64, source: bool, playing: bool, position: f64, at: Instant) -> Desktop {
        Desktop {
            track: Some(Arc::new(NowPlaying {
                video_id: "abc".into(),
                title: "Song".into(),
                artists: "Artist".into(),
                source: source.then(|| Source::File("/music/song.flac".into())),
            })),
            seq,
            clock: Clock { playing, position, at },
        }
    }

    #[test]
    fn mirror_follows_the_desktop() {
        let t0 = Instant::now();
        let s = Duration::from_secs;
        let mut m = Mirror::new(t0);
        assert_eq!(
            m.step(&desktop(1, true, true, 0.0, t0), t0),
            [Action::Load { at: 0.0, playing: true }]
        );
        // Steady playback, ticks with ordinary jitter: nothing to do.
        assert!(m.step(&desktop(1, true, true, 10.4, t0 + s(10)), t0 + s(10)).is_empty());
        // Pause, then resume.
        assert_eq!(m.step(&desktop(1, true, false, 12.0, t0 + s(12)), t0 + s(12)), [Action::Pause]);
        assert!(m.step(&desktop(1, true, false, 12.0, t0 + s(12)), t0 + s(40)).is_empty());
        assert_eq!(m.step(&desktop(1, true, true, 12.0, t0 + s(40)), t0 + s(40)), [Action::Play]);
        // A seek on the desktop.
        assert_eq!(
            m.step(&desktop(1, true, true, 90.0, t0 + s(41)), t0 + s(41)),
            [Action::Seek(90.0)]
        );
        // A seek while paused pauses and seeks.
        assert_eq!(
            m.step(&desktop(1, true, false, 5.0, t0 + s(42)), t0 + s(42)),
            [Action::Pause, Action::Seek(5.0)]
        );
        // The next track, and one with nothing to stream.
        assert_eq!(
            m.step(&desktop(2, true, false, 0.0, t0 + s(50)), t0 + s(50)),
            [Action::Load { at: 0.0, playing: false }]
        );
        assert_eq!(m.step(&desktop(3, false, true, 0.0, t0 + s(60)), t0 + s(60)), [Action::Stop]);
    }

    #[test]
    fn sources_are_remembered_per_track() {
        let headers = HashMap::from([("User-Agent".to_owned(), "x".to_owned())]);
        remember_source("yt", "http://127.0.0.1:1/t/a/1", &headers);
        remember_source("local", "/music/a.flac", &HashMap::new());
        remember_source("weird", "file:///music/a.flac", &HashMap::new());
        let Some(Source::Http { url, headers }) = source_for("yt") else { panic!() };
        assert_eq!(
            (url.as_str(), headers["user-agent"].to_str().unwrap()),
            ("http://127.0.0.1:1/t/a/1", "x")
        );
        assert_eq!(source_for("local"), Some(Source::File("/music/a.flac".into())));
        assert_eq!(source_for("weird"), None);
    }

    #[test]
    fn local_tracks_have_no_link() {
        assert_eq!(track_link("abc").as_deref(), Some("https://music.youtube.com/watch?v=abc"));
        assert_eq!(track_link("LOCAL:/music/a.flac"), None);
    }
}
