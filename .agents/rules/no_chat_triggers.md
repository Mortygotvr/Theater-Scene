# STRICT MANDATE: THERE ARE NO CHAT TRIGGERS

## Rule Definition
0. **Theater Scene Has No Access To Chat — Only Two Inputs**:
   - Theater Reader posts JSON with the event's data (platform, channel, event type, reward name, user…). That JSON is what fires a pose or a redeem.
   - The Control Panel fires a pose or a redeem directly with a Control Panel *command* (the "Theater Scene: Pose / Redeem" command, through the `theater_scene_fire` OBS procedure).
   - There is nothing else: no chat, no chat commands (`!word`) as triggers or defaults.
   - Vocabulary: in the Control Panel, a **command** is an action a Control Panel trigger runs (its "+ Add Command" list: Toggle Source, Update Text, module commands…). That is NOT a chat command. "Chat Command (!)" is one of the Control Panel's own trigger types; it belongs to the Control Panel and has nothing to do with Theater Scene's triggers.
1. **Triggers Are Theater Reader Events, Never Chat**:
   - Poses (Scene Coordinate Sets) and redeems fire ONLY from Theater Reader *events*: channel point redeems, subs, gift subs, bits/cheers, follows, raids, Super Chats, etc.
   - A trigger pattern is matched against the event's trigger string, which Theater Reader builds as `<Platform> <channel> <event> <details>`, e.g. `Twitch Mortygotvr redeem Throw Tomato`, `Twitch * redeem *`.
   - Chat and whisper messages NEVER fire a pose or a redeem. They only feed the chat overlay.
   - An event's message text (a redeem's typed input, a cheer message) is NEVER matched either.
2. **Never Add Chat Triggering**:
   - Do NOT add chat commands (`!pose1`, `!throw tomato`…), chat-message matching, or chat examples in hints, defaults, comments or tests.
   - Do NOT describe triggers as "chat triggers" anywhere.
3. **Where It Is Enforced**:
   - `Trust/src/studio_app.rs`: `process_trigger_event` (returns early for chat events via `is_chat_event`, and matches only `event.trigger`).
   - `Trust/src/studio_app.rs` test `test_chat_never_fires_triggers` must keep passing.
4. **Rationale**:
   - Otherwise any viewer could fire a pose or a redeem by typing its pattern in chat. The user has stated this rule repeatedly; code that wired chat into triggers had to be removed.
