use anyhow::{anyhow, Result};
use apex_music::{AsyncPlayer, Metadata as MetadataTrait, PlaybackStatus, PlayerEvent, Progress};
use futures_core::stream::Stream;
use std::future::Future;

use async_stream::stream;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use windows::Media::{
    Control,
    Control::{
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties,
        GlobalSystemMediaTransportControlsSessionPlaybackInfo,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    },
};

#[derive(Debug, Clone, Default)]
pub struct Metadata {
    title: String,
    artists: String,
    length: u64,
}

impl MetadataTrait for Metadata {
    fn title(&self) -> Result<String> {
        Ok(self.title.clone())
    }

    fn artists(&self) -> Result<String> {
        Ok(self.artists.clone())
    }

    fn length(&self) -> Result<u64> {
        Ok(self.length)
    }
}

pub struct Player {
    session_manager: GlobalSystemMediaTransportControlsSessionManager,
}

impl Player {
    pub fn new() -> Result<Self> {
        let session_manager =
            Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                .map_err(|_| anyhow!("Windows"))?
                .get()
                .map_err(|_| anyhow!("Windows"))?;

        Ok(Self { session_manager })
    }

    pub fn current_session(&self) -> Result<GlobalSystemMediaTransportControlsSession> {
        self.session_manager
            .GetCurrentSession()
            .map_err(|e| anyhow!("Couldn't get current session: {}", e))
    }

    pub async fn media_properties(
        &self,
    ) -> Result<GlobalSystemMediaTransportControlsSessionMediaProperties> {
        let session = self.current_session()?;
        let x = session
            .TryGetMediaPropertiesAsync()
            .map_err(|e| anyhow!("Couldn't get media properties: {}", e))?
            .await?;

        Ok(x)
    }

    pub async fn progress(&self) -> Result<Progress<Metadata>> {
        Ok(Progress {
            metadata: self.metadata().await?,
            position: self.position().await?,
            status: self.playback_status().await?,
        })
    }

    #[allow(unreachable_code, unused_variables)]
    pub async fn stream(&self) -> Result<impl Stream<Item = PlayerEvent>> {
        let mut timer = tokio::time::interval(Duration::from_millis(100));
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        Ok(stream! {
            loop {
                timer.tick().await;
                yield PlayerEvent::Timer;
            }
        })
    }
}
impl AsyncPlayer for Player {
    type Metadata = Metadata;

    type MetadataFuture<'b> = impl Future<Output = Result<Self::Metadata>> + 'b
    where
        Self: 'b;
    type NameFuture<'b> = impl Future<Output = String> + 'b
    where
        Self: 'b;
    type PlaybackStatusFuture<'b> = impl Future<Output = Result<PlaybackStatus>> + 'b
    where
        Self: 'b;
    type PositionFuture<'b> = impl Future<Output = Result<i64>> + 'b
    where
        Self: 'b;

    #[allow(clippy::needless_lifetimes)]
    fn metadata<'this>(&'this self) -> Self::MetadataFuture<'this> {
        async {
            let session = self.current_session();

            let length = match session {
                Ok(session) => {
                    let timeline = session.GetTimelineProperties().map_err(|_| anyhow!("Windows"))?;
                    let timespan = timeline.EndTime().map_err(|_| anyhow!("Windows"))?;

                    timespan.Duration as u64
                },
                Err(_) => 0,
            };

            let props = self.media_properties().await?;
            let title = props.Title()?.to_string_lossy();
            let artists = props.Artist()?.to_string_lossy();

            Ok(Metadata { title, artists, length })
        }
    }

    #[allow(clippy::needless_lifetimes)]
    fn playback_status<'this>(&'this self) -> Self::PlaybackStatusFuture<'this> {
        async {
            let session = self.current_session();
            let session = match session {
                Ok(session) => session,
                Err(_) => return Ok(PlaybackStatus::Stopped),
            };

            let playback: GlobalSystemMediaTransportControlsSessionPlaybackInfo =
                session.GetPlaybackInfo().map_err(|_| anyhow!("Windows"))?;

            let status = playback.PlaybackStatus().map_err(|_| anyhow!("Windows"))?;

            Ok(match status {
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => {
                    PlaybackStatus::Playing
                }
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => {
                    PlaybackStatus::Paused
                }
                _ => PlaybackStatus::Stopped,
            })
        }
    }

    #[allow(clippy::needless_lifetimes)]
    fn name<'this>(&'this self) -> Self::NameFuture<'this> {
        async {
            let session = self.current_session();
            let session = match session {
                Ok(session) => session,
                Err(_) => return String::from("windows-api"),
            };

            let name = match session.SourceAppUserModelId() {
                Ok(name) => name,
                Err(_) => return String::from("windows-api"),
            };

            name.to_string()
        }
    }

    #[allow(clippy::needless_lifetimes)]
    fn position<'this>(&'this self) -> Self::PositionFuture<'this> {
        async {
            let session = self.current_session();
            let session = match session {
                Ok(session) => session,
                Err(_) => return Ok(0),
            };
    
            let timeline = session.GetTimelineProperties().map_err(|_| anyhow!("Windows"))?;
            let position = timeline.Position().map_err(|_| anyhow!("Windows"))?;

            Ok(position.Duration)
        }
    }
}
