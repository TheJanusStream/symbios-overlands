//! `agent say` (#1417): the agent speaks in the room, through the chat
//! window's own send path.
//!
//! Held to the budget every receiver holds a speaker to
//! (`network::presence::ChatBudgets`): a burst, then a steady rate. A person
//! typing rarely meets it; an agent in a loop would, and past it every peer
//! silently drops the line. So the agent is refused here instead, and told
//! how long to wait.

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;
use serde_json::{Value, json};

use crate::config::ui::chat::{BURST_MESSAGES, MESSAGES_PER_SEC};
use crate::network::chat_send::{outgoing_chat_line, send_chat_line};
use crate::network::{ChatDelivery, LinkState};
use crate::player::emote::EmoteRequest;
use crate::protocol::OverlandsMessage;
use crate::state::{AppState, ChatHistory, LocalPlayer, RemotePeer};

/// How much the agent may still say before a peer would drop a line.
#[derive(Resource)]
pub(super) struct SpeechBudget {
    lines: f64,
    at: f64,
}

impl Default for SpeechBudget {
    fn default() -> Self {
        Self {
            lines: BURST_MESSAGES,
            at: 0.0,
        }
    }
}

impl SpeechBudget {
    /// Spend one line at `now` (seconds), or say how long until one is free.
    fn spend(&mut self, now: f64) -> Result<(), f64> {
        let elapsed = (now - self.at).max(0.0);
        self.lines = (self.lines + elapsed * MESSAGES_PER_SEC).min(BURST_MESSAGES);
        self.at = now;
        if self.lines >= 1.0 {
            self.lines -= 1.0;
            Ok(())
        } else {
            Err((1.0 - self.lines) / MESSAGES_PER_SEC)
        }
    }
}

type SayParams<'w, 's> = (
    Option<Res<'w, AtprotoSession>>,
    Res<'w, LinkState>,
    Query<'w, 's, (), With<RemotePeer>>,
    Query<'w, 's, Entity, With<LocalPlayer>>,
    ResMut<'w, ChatHistory>,
    MessageWriter<'w, EmoteRequest>,
    MessageWriter<'w, Broadcast<OverlandsMessage>>,
);

/// Say `raw` in the room the agent is in.
pub(super) fn say(world: &mut World, raw: &str) -> Result<Value, String> {
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return Err("the agent is not in a world yet".to_owned());
    }
    let text = outgoing_chat_line(raw).ok_or("there is nothing to say")?;
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    world
        .get_resource_or_init::<SpeechBudget>()
        .spend(now)
        .map_err(|wait| {
            format!(
                "speaking too fast: other players drop a speaker's lines past \
                 {BURST_MESSAGES} at once or {MESSAGES_PER_SEC} a second; wait \
                 {wait:.1} s"
            )
        })?;
    let mut state = SystemState::<SayParams>::new(world);
    let (session, link, peers, local, mut chat, mut emotes, mut broadcasts) = state
        .get_mut(world)
        .map_err(|e| format!("the world cannot take a line yet: {e}"))?;
    let delivery = link.delivery(peers.iter().count());
    send_chat_line(
        text.clone(),
        session.as_deref(),
        delivery,
        local.single().ok(),
        &mut chat,
        &mut emotes,
        &mut broadcasts,
    );
    state.apply(world);
    Ok(json!({ "said": text, "delivery": delivery_word(delivery) }))
}

/// What became of a line, for the agent: who heard it.
fn delivery_word(delivery: ChatDelivery) -> Value {
    match delivery {
        ChatDelivery::Reached(peers) => json!({ "reached": peers }),
        ChatDelivery::NobodyHere => json!("nobody_here"),
        ChatDelivery::NotConnected => json!("not_connected"),
        ChatDelivery::NotApplicable => json!(null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The burst, then the steady rate - the budget every receiver applies.
    #[test]
    fn the_budget_allows_a_burst_then_the_rate() {
        let mut budget = SpeechBudget::default();
        for n in 0..BURST_MESSAGES as usize {
            assert!(budget.spend(0.0).is_ok(), "line {n} of the burst");
        }
        let wait = budget.spend(0.0).expect_err("the burst is spent");
        assert!((wait - 1.0 / MESSAGES_PER_SEC).abs() < 1e-9, "{wait}");
        assert!(budget.spend(1.0 / MESSAGES_PER_SEC).is_ok(), "one refilled");
    }

    fn world_in(state: AppState) -> World {
        let mut world = World::new();
        world.insert_resource(State::new(state));
        world.insert_resource(Time::<Real>::default());
        world.insert_resource(LinkState::default());
        world.init_resource::<ChatHistory>();
        world.init_resource::<Messages<EmoteRequest>>();
        world.init_resource::<Messages<Broadcast<OverlandsMessage>>>();
        world
    }

    /// A line goes out on the wire and into the agent's own history, and the
    /// answer says it reached nobody - the link is down.
    #[test]
    fn a_line_is_sent_and_its_delivery_reported() {
        let mut world = world_in(AppState::InGame);

        let said = say(&mut world, "  hello\nall ").expect("said");

        assert_eq!(said["said"], "hello all");
        assert_eq!(said["delivery"], "not_connected");
        let sent = world.resource::<Messages<Broadcast<OverlandsMessage>>>();
        let mut cursor = sent.get_cursor();
        let lines: Vec<_> = cursor
            .read(sent)
            .map(|b| match &b.payload {
                OverlandsMessage::Chat { text } => text.clone(),
                other => panic!("not chat: {other:?}"),
            })
            .collect();
        assert_eq!(lines, ["hello all"]);
        let history = world.resource::<ChatHistory>();
        assert_eq!(
            history.messages.last().map(|m| m.text.as_str()),
            Some("hello all")
        );
    }

    #[test]
    fn nothing_is_said_outside_a_world() {
        let mut world = world_in(AppState::Loading);
        assert!(say(&mut world, "hello").is_err());
        assert!(world.resource::<ChatHistory>().messages.is_empty());
    }

    #[test]
    fn a_blank_line_is_refused_without_spending_the_budget() {
        let mut world = world_in(AppState::InGame);
        assert!(say(&mut world, " \n ").is_err());
        assert!(world.get_resource::<SpeechBudget>().is_none());
    }
}
