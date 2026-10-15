//! `/model` (alias `/m`) — switch model + (optionally) reasoning effort.
//! Chained autocomplete: pick a reasoning-supported model → trailing space
//! re-opens the dropdown into a `low|medium|high|xhigh` sub-menu.
//!
//! Multi-provider display: managed platform models (`{platform}/{model}`) show
//! the provider on the right. When several rows share a display name (e.g.
//! GLM-5.2 on Z.AI Coding Plan CN and on Ollama), the left label is
//! disambiguated and `insert_text` uses the catalog id so selection is unique.

use agent_client_protocol as acp;
// hyper: BYOK platform lock metadata + managed model keys; official:
// ReasoningEffortOption for the effort sub-menu rows.
use xai_grok_models::parse_managed_model_key;
use xai_grok_shell::sampling::types::{
    PlatformLockMeta, ReasoningEffortOption, supports_reasoning_effort_meta,
};

// Official supplies slash_meta!, ModelChoice and the context_window helpers;
// hyper adds platform_lock for the BYOK lock gate.
use crate::acp::model_state::{ModelState, platform_lock};
use crate::app::actions::{Action, ModelChoice};
use crate::slash::command::{
    AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand, slash_meta,
};
use crate::slash::commands::context_window::{
    format_window, parse_window_token, supported_window, unknown_window_error, window_arg_items,
};
use crate::slash::commands::effort_levels::build_effort_arg_items;

/// Switch the active model (and optionally its reasoning effort).
pub struct ModelCommand;

impl SlashCommand for ModelCommand {
    slash_meta! {
        name: "model",
        aliases: ["m"],
        description: "Switch the active model",
        usage: "/model <name> [window] [effort]",
        takes_args: true,
        args_required: true,
        session_scoped: true,
        // The dashboard offers `/model` to pick the model for the next spawned agent (intercepted in `dispatch_dashboard_dispatch_slash`).
        offered_when_session_less: true,
        arg_placeholder: "<model> [window] [effort]",
    }

    fn suggest_args(&self, ctx: &AppCtx, args_query: &str) -> Option<Vec<ArgItem>> {
        if ctx.models.is_empty() {
            return None;
        }

        match chained_phase(ctx.models, args_query) {
            Some(ChainedPhase::Window { model_id, prefix }) => {
                Some(build_window_items(ctx.models, &model_id, &prefix))
            }
            Some(ChainedPhase::Effort { model_id, prefix }) => {
                Some(build_effort_items(ctx.models, &model_id, &prefix))
            }
            Some(ChainedPhase::Done) => None,
            None => Some(build_model_items(ctx.models)),
        }
    }

    fn preselected_arg(&self, ctx: &AppCtx, args_query: &str) -> Option<String> {
        // A typed filter leaves the opening row to the match ranking
        let fresh = |prefix: &str| args_query.trim_end().eq_ignore_ascii_case(prefix);
        match chained_phase(ctx.models, args_query)? {
            ChainedPhase::Window { model_id, prefix } if fresh(&prefix) => {
                let window = ctx.models.window_after_switch_to(&model_id)?;
                Some(window_insert_text(ctx.models, &model_id, &prefix, window))
            }
            ChainedPhase::Effort { model_id, prefix } if fresh(&prefix) => {
                let option = ctx.models.preselected_effort_option_for(&model_id)?;
                // hyper: preselect the same unique-token row `suggest_args` builds.
                Some(effort_insert_text(
                    &effort_insert_prefix(ctx.models, &model_id, &prefix),
                    &option,
                ))
            }
            ChainedPhase::Window { .. } | ChainedPhase::Effort { .. } | ChainedPhase::Done => None,
        }
    }

    fn run(&self, ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return CommandResult::Error("Usage: /model <name> [window] [effort]".into());
        }

        // Prefer an exact full-string catalog match first. Model display names often contain spaces ("Grok 4.5").
        // If we split on the last token first, a shorter catalog entry ("Grok") would steal the prefix and treat "4.5" as an effort level
        if let Some(id) = ctx.models.resolve_by_name_or_id(trimmed) {
            if let Some(lock) = locked_model(ctx.models, &id) {
                return CommandResult::Error(lock_message(&lock, trimmed));
            }
            return CommandResult::Action(Action::SetDefaultModel(id));
        }
        if let Some(msg) = ambiguous_model_message(ctx.models, trimmed) {
            return CommandResult::Error(msg);
        }

        // hyper: a locked platform model followed by a trailing token
        // surfaces the setup error — never "Unknown model: …". (Locked rows
        // never chain, so the split below cannot see them.)
        if let Some((prefix, _)) = split_trailing_token(trimmed)
            && let Some(id) = resolve_model(ctx.models, prefix)
            && let Some(lock) = locked_model(ctx.models, &id)
        {
            return CommandResult::Error(lock_message(&lock, prefix));
        }

        // A trailing window or effort after the model is a session switch
        // (the leading window token is official's chained parsing; hyper's
        // lock gate above keeps the BYOK setup error for locked rows).
        if let Some((id, rest)) = split_model_rest(ctx.models, trimmed) {
            let windows = ctx.models.context_window_options_for(&id);
            let (window, effort_token) = match split_window_effort(rest, &windows) {
                Ok(split) => split,
                Err(message) => return CommandResult::Error(message),
            };

            // A window alone keeps the current model's effort, like `/context-window`
            if let Some(window) = window
                && effort_token.is_empty()
            {
                let effort = ctx
                    .models
                    .reasoning_effort
                    .filter(|_| ctx.models.current.as_ref() == Some(&id));
                return CommandResult::Action(Action::SwitchModel(ModelChoice {
                    model_id: id,
                    effort,
                    context_window_selection: Some(window),
                }));
            }
            return match ctx.models.resolve_effort_for_model(&id, effort_token) {
                Ok(effort) => CommandResult::Action(Action::SwitchModel(ModelChoice {
                    model_id: id,
                    effort: Some(effort),
                    context_window_selection: window,
                })),
                Err(err) => CommandResult::Error(err.message()),
            };
        }
        if let Some((prefix, _)) = split_trailing_token(trimmed)
            && let Some(msg) = ambiguous_model_message(ctx.models, prefix)
        {
            return CommandResult::Error(msg);
        }

        CommandResult::Error(format!("Unknown model: {trimmed}"))
    }
}

// hyper: locked platform model (BYOK) helpers — official added nothing here.
/// Lock metadata when `id` is a credential-less managed platform model.
fn locked_model(models: &ModelState, id: &acp::ModelId) -> Option<PlatformLockMeta> {
    models.available.get(id).and_then(platform_lock)
}

/// User-facing rejection for picking a locked model: what to configure.
fn lock_message(lock: &PlatformLockMeta, requested: &str) -> String {
    let provider = if lock.platform_name.is_empty() {
        lock.platform.as_str()
    } else {
        lock.platform_name.as_str()
    };
    format!(
        "'{requested}' is provided by {provider}, which is not configured yet. \
         To enable it: {}. See /providers for all platforms.",
        lock.setup_hint
    )
}

/// Look up a model by case-insensitive display name OR model id match.
fn resolve_model(models: &ModelState, name: &str) -> Option<acp::ModelId> {
    models.resolve_by_name_or_id(name)
}

fn supports_reasoning_effort(info: &acp::ModelInfo) -> bool {
    supports_reasoning_effort_meta(info.meta.as_ref())
}

/// The arg picker title for the `/model` sub-menu that `args_query` has reached.
pub(crate) fn picker_title(models: &ModelState, args_query: &str) -> &'static str {
    match chained_phase(models, args_query) {
        Some(ChainedPhase::Window { .. }) => "Pick context window",
        Some(ChainedPhase::Effort { .. }) => "Pick reasoning effort",
        Some(ChainedPhase::Done) | None => "Pick model",
    }
}

/// Provider label for a catalog row: lock meta, managed platform key, or none.
fn provider_label(id: &acp::ModelId, info: &acp::ModelInfo) -> Option<String> {
    if let Some(lock) = platform_lock(info) {
        let name = if lock.platform_name.is_empty() {
            lock.platform
        } else {
            lock.platform_name
        };
        return Some(name);
    }
    parse_managed_model_key(id.0.as_ref()).map(|(platform, _)| platform.display_name().to_string())
}

/// Whether `description` is the generic Pi catalog source stamp (not useful in
/// the picker UI — the provider column already conveys the same signal).
fn is_generic_pi_catalog_description(description: &str) -> bool {
    description
        .trim()
        .to_ascii_lowercase()
        .starts_with("official pi catalog")
}

/// Token inserted into `/model …` so selection stays unambiguous.
///
/// Managed platform rows (`{platform}/{model}`) always use the catalog id —
/// several providers can ship the same display name (GLM-5.2 on Z.AI CN and
/// Ollama). Non-managed rows use the friendly display name unless that name
/// collides with another catalog entry.
fn model_arg_token(id: &acp::ModelId, info: &acp::ModelInfo, name_collides: bool) -> String {
    if parse_managed_model_key(id.0.as_ref()).is_some() || name_collides {
        id.0.to_string()
    } else {
        info.name.clone()
    }
}

/// Right-column text: prefer a human provider name for managed/locked rows;
/// otherwise keep a non-generic catalog description when present.
fn model_row_description(
    info: &acp::ModelInfo,
    provider: Option<&str>,
    locked_setup_hint: Option<&str>,
) -> String {
    if let Some(hint) = locked_setup_hint {
        return hint.to_string();
    }
    if let Some(provider) = provider {
        return provider.to_string();
    }
    info.description
        .as_deref()
        .filter(|d| !d.is_empty() && !is_generic_pi_catalog_description(d))
        .unwrap_or("")
        .to_string()
}

/// Error when the typed display name matches multiple configured providers.
fn ambiguous_model_message(models: &ModelState, query: &str) -> Option<String> {
    let matches = models.ids_matching_name(query);
    if matches.len() < 2 {
        return None;
    }
    let mut options: Vec<String> = matches
        .iter()
        .map(|id| {
            let provider = models
                .available
                .get(id)
                .and_then(|info| provider_label(id, info))
                .unwrap_or_else(|| id.0.to_string());
            format!("  {provider}  →  /model {}", id.0)
        })
        .collect();
    options.sort();
    Some(format!(
        "'{query}' matches multiple providers:\n{}\nPick one by id.",
        options.join("\n")
    ))
}

/// Split `args` into `(prefix, last_token)` on the final whitespace run.
/// Returns `None` when there is no interior whitespace to split on. The token is
/// resolved to an effort against the picked model's options by the caller.
fn split_trailing_token(args: &str) -> Option<(&str, &str)> {
    let (prefix, last) = args.rsplit_once(char::is_whitespace)?;
    let prefix = prefix.trim_end();
    if prefix.is_empty() || last.is_empty() {
        return None;
    }
    Some((prefix, last))
}

// Official's chained phase machine replaces hyper's `detect_effort_phase`;
// hyper's unique-token / locked-row rules are folded into
// `longest_chained_prefix` below.
/// Which chained sub-menu the typed args have reached.
enum ChainedPhase {
    Window {
        model_id: acp::ModelId,
        prefix: String,
    },
    Effort {
        model_id: acp::ModelId,
        prefix: String,
    },
    Done,
}

/// The sub-menu for `args_query`, or `None` while the user is still picking a model.
fn chained_phase(models: &ModelState, args_query: &str) -> Option<ChainedPhase> {
    let (id, key, rest) = longest_chained_prefix(models, args_query)?;
    let model_id = id.clone();
    let windows = models.context_window_options_for(&model_id);
    let reasoning = models
        .available
        .get(&model_id)
        .is_some_and(supports_reasoning_effort);

    let phase = match committed_first_token(rest) {
        // A committed window advances the chain to the effort phase
        Some(first)
            if parse_window_token(first).is_some_and(|window| windows.contains(&window)) =>
        {
            if reasoning {
                ChainedPhase::Effort {
                    model_id,
                    prefix: format!("{key} {first}"),
                }
            } else {
                ChainedPhase::Done
            }
        }
        // Any other committed token is a typed effort
        Some(_) if reasoning => ChainedPhase::Effort {
            model_id,
            prefix: key.to_string(),
        },
        // A longer model name may still match ("Foo Bar" after "Foo")
        Some(_) => return None,
        None if windows.len() > 1 => {
            // A partial token that can't start any window row ("Grok 4.7 hi") is a typed effort
            if starts_a_window(&windows, rest.trim_start()) {
                ChainedPhase::Window {
                    model_id,
                    prefix: key.to_string(),
                }
            } else if reasoning {
                ChainedPhase::Effort {
                    model_id,
                    prefix: key.to_string(),
                }
            } else {
                return None;
            }
        }
        None if reasoning => ChainedPhase::Effort {
            model_id,
            prefix: key.to_string(),
        },
        None => ChainedPhase::Done,
    };
    Some(phase)
}

/// The first token is committed once whitespace follows it.
fn committed_first_token(rest: &str) -> Option<&str> {
    let rest_trimmed = rest.trim_start();
    match rest_trimmed.split_once(char::is_whitespace) {
        Some((first, _)) => Some(first),
        None if !rest_trimmed.is_empty() && rest.ends_with(char::is_whitespace) => {
            Some(rest_trimmed)
        }
        None => None,
    }
}

/// Whether a partial token can still grow into one of the window rows (an empty partial always can).
fn starts_a_window(windows: &[u64], partial: &str) -> bool {
    let partial = partial.to_ascii_lowercase();
    windows.iter().any(|&window| {
        format_window(window).starts_with(&partial) || window.to_string().starts_with(&partial)
    })
}

fn split_on_model_key<'a>(args: &'a str, key: &str) -> Option<&'a str> {
    let rest = args.get(key.len()..)?;
    if args
        .get(..key.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(key))
        && rest.starts_with(char::is_whitespace)
    {
        Some(rest)
    } else {
        None
    }
}

/// Whether picking this model chains into a sub-menu (windows or effort).
fn has_chained_args(models: &ModelState, id: &acp::ModelId, info: &acp::ModelInfo) -> bool {
    supports_reasoning_effort(info) || models.context_window_options_for(id).len() > 1
}

/// The longest name or id that prefixes `args` among models with a sub-menu, and the text after it.
fn longest_chained_prefix<'a>(
    models: &'a ModelState,
    args: &'a str,
) -> Option<(&'a acp::ModelId, &'a str, &'a str)> {
    // hyper: display-name counts cover the full catalog (locked rows
    // included) so an ambiguous name never chains — `run()` lists the
    // matching providers instead and only the catalog id disambiguates.
    let mut name_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for info in models.available.values() {
        *name_counts
            .entry(info.name.to_ascii_lowercase())
            .or_default() += 1;
    }

    let mut best: Option<(&acp::ModelId, &str, &str)> = None;
    for (id, info) in models
        .available
        .iter()
        .filter(|(id, info)| has_chained_args(models, id, info))
        // hyper: locked platform models never chain — picking one must hit
        // the lock error in `run()`, not open a sub-menu.
        .filter(|(_, info)| platform_lock(info).is_none())
    {
        let name_collides = name_counts
            .get(&info.name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
            > 1;
        let name = info.name.as_str();
        let id_str = id.0.as_ref();
        for (idx, &key) in [name, id_str].iter().enumerate() {
            // An ambiguous display name never chains; the catalog id stays unique.
            if idx == 0 && name_collides {
                continue;
            }
            if best.is_some_and(|(_, prev, _)| prev.len() >= key.len()) {
                continue;
            }
            if let Some(rest) = split_on_model_key(args, key) {
                best = Some((id, key, rest));
            }
        }
    }
    best
}

fn split_model_rest<'a>(models: &'a ModelState, args: &'a str) -> Option<(acp::ModelId, &'a str)> {
    let (id, _, rest) = longest_chained_prefix(models, args)?;
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some((id.clone(), rest))
    }
}

/// A window is always the first token, and the remaining words are the effort.
fn split_window_effort<'a>(
    rest: &'a str,
    windows: &[u64],
) -> Result<(Option<std::num::NonZeroU64>, &'a str), String> {
    let (first, tail) = match rest.split_once(char::is_whitespace) {
        Some((first, tail)) => (first, tail.trim()),
        None => (rest, ""),
    };
    if windows.is_empty() || parse_window_token(first).is_none() {
        return Ok((None, rest));
    }
    match supported_window(first, windows) {
        Some(window) => Ok((Some(window), tail)),
        None => Err(unknown_window_error(first, windows)),
    }
}

/// One row per logical model.
fn build_model_items(models: &ModelState) -> Vec<ArgItem> {
    let current_id = models.current.as_ref();
    let mut name_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for info in models.available.values() {
        *name_counts
            .entry(info.name.to_ascii_lowercase())
            .or_default() += 1;
    }

    let mut items: Vec<ArgItem> = Vec::with_capacity(models.available.len());
    let mut locked_items: Vec<ArgItem> = Vec::new();
    for (id, info) in &models.available {
        let is_current = current_id == Some(id);
        // hyper: per-row provider label, unique insert token, and locked
        // (credential-less BYOK) rows sorted last with a 🔒 marker.
        let name_collides = name_counts
            .get(&info.name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
            > 1;
        let provider = provider_label(id, info);
        let token = model_arg_token(id, info, name_collides);

        if let Some(lock) = platform_lock(info) {
            let provider = provider.as_deref().unwrap_or(lock.platform.as_str());
            locked_items.push(ArgItem {
                display: format!("🔒 {} — {provider}", info.name),
                match_text: format!("{} {provider} {}", info.name, id.0),
                // Catalog id keeps locked multi-provider rows distinct if the
                // user types the token before credentials are configured.
                insert_text: token,
                description: lock.setup_hint.clone(),
                locked: true,
                action_id: Some(id.0.to_string()),
                hidden: false,
            });
            continue;
        }

        let supports = supports_reasoning_effort(info);
        let display = match (name_collides, provider.as_deref(), is_current) {
            (true, Some(p), true) => format!("{} — {p} (current)", info.name),
            (true, Some(p), false) => format!("{} — {p}", info.name),
            (true, None, true) => format!("{} — {} (current)", info.name, id.0),
            (true, None, false) => format!("{} — {}", info.name, id.0),
            (false, _, true) => format!("{} (current)", info.name),
            (false, _, false) => info.name.clone(),
        };

        // Trailing space when picking this model chains into a sub-menu
        // (window or effort): signals "more input expected" to the prompt
        // widget so Enter advances to the next phase instead of submitting.
        // hyper: the insert text is the unique token (catalog id for managed
        // or name-colliding rows), not the raw display name.
        let chains = supports || models.context_window_options_for(id).len() > 1;
        let insert_text = if chains { format!("{token} ") } else { token };

        let match_text = match provider.as_deref() {
            Some(p) => format!("{} {p} {}", info.name, id.0),
            None => info.name.clone(),
        };

        items.push(ArgItem {
            display,
            match_text,
            insert_text,
            description: model_row_description(info, provider.as_deref(), None),
            locked: false,
            action_id: Some(id.0.to_string()),
            hidden: false,
        });
    }
    items.extend(locked_items);
    items
}

/// One row per selectable window for the `/model` chained window phase.
fn build_window_items(models: &ModelState, model_id: &acp::ModelId, prefix: &str) -> Vec<ArgItem> {
    let windows = models.context_window_options_for(model_id);
    // Only the current model has an active window
    let active = (models.current.as_ref() == Some(model_id))
        .then(|| models.get_context_window())
        .flatten();
    let default = models.model_default_window(model_id);
    window_arg_items(&windows, active, default, |_, window| {
        window_insert_text(models, model_id, prefix, window)
    })
}

fn window_insert_text(
    models: &ModelState,
    model_id: &acp::ModelId,
    prefix: &str,
    window: u64,
) -> String {
    let chains_to_effort = models
        .available
        .get(model_id)
        .is_some_and(supports_reasoning_effort);
    let label = format_window(window);
    if chains_to_effort {
        format!("{prefix} {label} ")
    } else {
        format!("{prefix} {label}")
    }
}

/// One row per effort level for the `/model` chained effort phase.
/// `prefix` is the name or catalog id the user typed, plus an optional
/// committed window token (official's chained phase). Rows complete with the
/// unique model token — the catalog id for managed or name-colliding rows —
/// so selecting a row inserts the same token the model phase uses (hyper).
fn build_effort_items(models: &ModelState, model_id: &acp::ModelId, prefix: &str) -> Vec<ArgItem> {
    if !models.available.contains_key(model_id) {
        return Vec::new();
    }
    let insert_prefix = effort_insert_prefix(models, model_id, prefix);
    let is_current_model = models.current.as_ref() == Some(model_id);
    let options = models.reasoning_effort_options_for(model_id);
    build_effort_arg_items(
        &options,
        models.reasoning_effort,
        is_current_model,
        |option| format!("{insert_prefix} {}", option.id),
    )
}

fn effort_insert_text(prefix: &str, option: &ReasoningEffortOption) -> String {
    format!("{prefix} {}", option.id)
}

/// The text effort rows insert: the unique model token (catalog id for
/// managed or name-colliding rows) with any committed window token that
/// trails the model key in `prefix` preserved.
fn effort_insert_prefix(models: &ModelState, model_id: &acp::ModelId, prefix: &str) -> String {
    let Some(info) = models.available.get(model_id) else {
        return prefix.to_string();
    };
    let name_collides = models
        .available
        .values()
        .filter(|i| i.name.eq_ignore_ascii_case(&info.name))
        .count()
        > 1;
    let token = model_arg_token(model_id, info, name_collides);
    // The chain matched the longest model key, so the longer key leads
    // `prefix`; try it first.
    let mut keys = [info.name.as_str(), model_id.0.as_ref()];
    keys.sort_by_key(|key| std::cmp::Reverse(key.len()));
    for key in keys {
        let Some(head) = prefix.get(..key.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(key) {
            continue;
        }
        let rest = &prefix[key.len()..];
        if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            return format!("{token}{rest}");
        }
    }
    prefix.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use std::sync::Arc;
    use xai_grok_shell::sampling::types::ReasoningEffort;
    use xai_grok_test_support::acp_fixtures;

    fn model_with_reasoning(id: &str, name: &str) -> (acp::ModelId, acp::ModelInfo) {
        let id = acp::ModelId::new(Arc::from(id));
        let mut meta = serde_json::Map::new();
        meta.insert(
            "supportsReasoningEffort".into(),
            serde_json::Value::Bool(true),
        );
        let info = acp::ModelInfo::new(id.clone(), name.to_string())
            .meta(serde_json::Value::Object(meta).as_object().cloned());
        (id, info)
    }

    fn plain_model(id: &str, name: &str) -> (acp::ModelId, acp::ModelInfo) {
        let id = acp::ModelId::new(Arc::from(id));
        let info = acp::ModelInfo::new(id.clone(), name.to_string());
        (id, info)
    }

    static EMPTY_BUNDLE: crate::app::bundle::BundleState = crate::app::bundle::BundleState {
        has_cache: false,
        version: String::new(),
        personas: Vec::new(),
        roles: Vec::new(),
        agents: Vec::new(),
        skills: Vec::new(),
        persona_details: Vec::new(),
        role_details: Vec::new(),
    };

    fn dummy_exec_ctx(models: &ModelState) -> CommandExecCtx<'_> {
        CommandExecCtx {
            models,
            session_id: None,
            bundle_state: &EMPTY_BUNDLE,
            screen_mode: crate::app::ScreenMode::Inline,
            billing_surface_visible: true,
            usage_command_visible: true,
            pager_state: crate::settings::PagerLocalSnapshot {
                multiline_mode: false,
                yolo_mode: false,
                ..crate::settings::PagerLocalSnapshot::default()
            },
        }
    }

    fn model_with_windows_and_reasoning(id: &str, name: &str) -> (acp::ModelId, acp::ModelInfo) {
        let info = acp_fixtures::model_info_with_meta(
            id,
            name,
            serde_json::json!({
                "supportsReasoningEffort": true,
                "totalContextTokens": 256_000,
                "contextWindows": [256_000, 500_000],
            }),
        );
        (acp_fixtures::model_id(id), info)
    }

    fn app_ctx(models: &ModelState) -> AppCtx<'_> {
        AppCtx {
            models,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        }
    }

    #[test]
    fn split_model_rest_keeps_a_multi_word_label() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("grok-4.7", "Grok 4.7");
        state.available.insert(id.clone(), info);
        assert_eq!(
            split_model_rest(&state, "Grok 4.7 Extra High")
                .map(|(model, token)| { (model.0.to_string(), token.to_string()) }),
            Some(("grok-4.7".to_string(), "Extra High".to_string()))
        );
        assert_eq!(
            split_model_rest(&state, "Grok 4.7 high").map(|(_, token)| token),
            Some("high")
        );
        assert!(split_model_rest(&state, "Grok 4.7").is_none());
        assert_eq!(
            split_model_rest(&state, "grok-4.7 Extra High")
                .map(|(model, token)| (model.0.to_string(), token.to_string())),
            Some(("grok-4.7".to_string(), "Extra High".to_string()))
        );
    }

    #[test]
    fn window_phase_sits_between_model_and_effort() {
        let mut state = ModelState::default();
        let (id, info) = model_with_windows_and_reasoning("grok-4.7", "Grok 4.7");
        state.available.insert(id, info);
        let cmd = ModelCommand;
        let ctx = app_ctx(&state);

        // A trailing space after the model opens the window phase
        let items = cmd
            .suggest_args(&ctx, "Grok 4.7 ")
            .expect("window rows after the model");
        let [first, second] = items.as_slice() else {
            panic!("expected 2 window rows: {items:?}");
        };
        assert_eq!(first.display, "256k");
        // The trailing space chains into the effort phase
        assert_eq!(second.insert_text, "Grok 4.7 500k ");

        // A committed window advances to the effort phase with the longer prefix
        let items = cmd
            .suggest_args(&ctx, "Grok 4.7 500k ")
            .expect("effort rows after a committed window");
        assert_eq!(
            items.first().map(|item| item.insert_text.as_str()),
            Some("Grok 4.7 500k xhigh")
        );

        // A partial that can't start any window row falls back to the effort rows
        let items = cmd
            .suggest_args(&ctx, "Grok 4.7 hi")
            .expect("effort rows for a typed effort filter");
        assert!(
            items.iter().any(|item| item.insert_text == "Grok 4.7 high"),
            "typed effort must keep the effort rows up: {items:?}"
        );

        // The fresh window menu preselects the default
        assert_eq!(
            cmd.preselected_arg(&ctx, "Grok 4.7 ").as_deref(),
            Some("Grok 4.7 256k ")
        );
    }

    #[test]
    fn picker_title_follows_the_chained_phase() {
        let mut state = ModelState::default();
        let (id, info) = model_with_windows_and_reasoning("grok-4.7", "Grok 4.7");
        state.available.insert(id, info);

        assert_eq!(picker_title(&state, ""), "Pick model");
        assert_eq!(picker_title(&state, "Grok 4.7 "), "Pick context window");
        assert_eq!(
            picker_title(&state, "Grok 4.7 500k "),
            "Pick reasoning effort"
        );
    }

    #[test]
    fn a_non_window_partial_keeps_the_model_rows() {
        let mut state = ModelState::default();
        let info = acp_fixtures::model_info_with_meta(
            "foo",
            "Foo",
            serde_json::json!({ "contextWindows": [256_000, 500_000] }),
        );
        state.available.insert(acp_fixtures::model_id("foo"), info);
        let (id, info) = plain_model("foo-bar", "Foo Bar");
        state.available.insert(id, info);
        let ctx = app_ctx(&state);

        let items = ModelCommand
            .suggest_args(&ctx, "Foo B")
            .expect("model rows while the partial can't start a window");

        assert!(
            items.iter().any(|item| item.insert_text == "Foo Bar"),
            "items={items:?}"
        );
    }

    #[test]
    fn run_parses_window_and_effort_after_the_model() {
        let mut state = ModelState::default();
        let (id, info) = model_with_windows_and_reasoning("grok-4.7", "Grok 4.7");
        state.available.insert(id, info);
        let mut ctx = dummy_exec_ctx(&state);

        match ModelCommand.run(&mut ctx, "Grok 4.7 500k") {
            CommandResult::Action(Action::SwitchModel(ModelChoice {
                model_id,
                effort,
                context_window_selection,
            })) => {
                assert_eq!(model_id.0.as_ref(), "grok-4.7");
                assert_eq!(
                    effort, None,
                    "a window alone on another model sends no effort"
                );
                assert_eq!(context_window_selection, NonZeroU64::new(500_000));
            }
            other => panic!("expected window-only switch, got {other:?}"),
        }

        match ModelCommand.run(&mut ctx, "Grok 4.7 500k high") {
            CommandResult::Action(Action::SwitchModel(ModelChoice {
                model_id,
                effort,
                context_window_selection,
            })) => {
                assert_eq!(model_id.0.as_ref(), "grok-4.7");
                assert_eq!(effort, Some(ReasoningEffort::High));
                assert_eq!(context_window_selection, NonZeroU64::new(500_000));
            }
            other => panic!("expected window+effort switch, got {other:?}"),
        }

        // An effort with no window leaves the window unset
        match ModelCommand.run(&mut ctx, "Grok 4.7 high") {
            CommandResult::Action(Action::SwitchModel(ModelChoice {
                effort,
                context_window_selection,
                ..
            })) => {
                assert_eq!(effort, Some(ReasoningEffort::High));
                assert_eq!(context_window_selection, None);
            }
            other => panic!("expected effort-only switch, got {other:?}"),
        }

        // A parseable but unoffered window gets the window error with the offered list
        match ModelCommand.run(&mut ctx, "Grok 4.7 1m") {
            CommandResult::Error(msg) => {
                assert!(msg.contains("unknown context window '1m'"), "msg={msg}");
                assert!(msg.contains("256k, 500k"), "msg={msg}");
            }
            other => panic!("expected window error, got {other:?}"),
        }
    }

    #[test]
    fn window_only_pick_keeps_the_current_models_effort() {
        let mut state = ModelState::default();
        let (id, info) = model_with_windows_and_reasoning("grok-4.7", "Grok 4.7");
        state.available.insert(id.clone(), info);
        state.current = Some(id);
        state.reasoning_effort = Some(ReasoningEffort::Low);
        let mut ctx = dummy_exec_ctx(&state);

        let result = ModelCommand.run(&mut ctx, "Grok 4.7 500k");

        assert!(
            matches!(
                result,
                CommandResult::Action(Action::SwitchModel(ModelChoice {
                    effort: Some(ReasoningEffort::Low),
                    ..
                }))
            ),
            "got {result:?}"
        );
    }

    #[test]
    fn picker_preselects_the_window_the_switch_uses() {
        let mut state = ModelState::default();
        let (current, current_info) = model_with_windows_and_reasoning("grok-4.7", "Grok 4.7");
        let (listed, listed_info) = model_with_windows_and_reasoning("grok-4.8", "Grok 4.8");
        let unlisted_info = acp_fixtures::model_info_with_meta(
            "grok-4.5",
            "Grok 4.5",
            serde_json::json!({
                "supportsReasoningEffort": true,
                "totalContextTokens": 256_000,
                "contextWindows": [128_000, 256_000],
            }),
        );
        state.available.insert(current.clone(), current_info);
        state.available.insert(listed, listed_info);
        state
            .available
            .insert(acp_fixtures::model_id("grok-4.5"), unlisted_info);
        state.current = Some(current);
        state.context_window_selection = Some(500_000);
        let ctx = app_ctx(&state);

        let listed_row = ModelCommand.preselected_arg(&ctx, "Grok 4.8 ");
        let unlisted_row = ModelCommand.preselected_arg(&ctx, "Grok 4.5 ");

        assert_eq!(listed_row.as_deref(), Some("Grok 4.8 500k "));
        assert_eq!(unlisted_row.as_deref(), Some("Grok 4.5 256k "));
    }

    #[test]
    fn empty_query_returns_one_row_per_logical_model() {
        let mut state = ModelState::default();
        let (rid, rinfo) = model_with_reasoning("reasoning-x", "Reasoning X");
        let (pid, pinfo) = plain_model("grok-4.5", "Grok 4.5");
        state.available.insert(rid, rinfo);
        state.available.insert(pid, pinfo);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        let items = cmd.suggest_args(&ctx, "").unwrap();
        assert_eq!(items.len(), 2, "model phase: one row per logical model");

        // A reasoning model has a trailing space in insert_text
        // The prompt widget reads it to keep the dropdown open after Enter so the effort sub-menu can render
        let reasoning = items
            .iter()
            .find(|i| i.match_text == "Reasoning X")
            .unwrap();
        assert_eq!(reasoning.insert_text, "Reasoning X ");

        // A plain model has no trailing space, so Enter commits immediately
        let plain = items.iter().find(|i| i.match_text == "Grok 4.5").unwrap();
        assert_eq!(plain.insert_text, "Grok 4.5");
    }

    #[test]
    fn trailing_space_after_reasoning_model_enters_effort_phase() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id, info);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        // The args query has a trailing space, so this is the effort phase
        // Items come out ordered xhigh to low (strongest first) per EFFORT_LEVELS
        let items = cmd.suggest_args(&ctx, "Reasoning X ").unwrap();
        assert_eq!(items.len(), 4);
        let [a, b, c, d] = items.as_slice() else {
            panic!("expected 4 items: {items:?}");
        };
        assert_eq!(a.insert_text, "Reasoning X xhigh");
        assert_eq!(b.insert_text, "Reasoning X high");
        assert_eq!(c.insert_text, "Reasoning X medium");
        assert_eq!(d.insert_text, "Reasoning X low");
        // Display is just the level so the user sees a clean column.
        assert_eq!(a.display, "xhigh");
        // match_text carries the sort-key prefix that forces the matcher's alphabetical tiebreak to render rows in EFFORT_LEVELS order
        assert!(a.match_text.starts_with("a "));
        assert!(d.match_text.starts_with("d "));
    }

    #[test]
    fn preselected_arg_targets_default_row_only_for_fresh_effort_menu() {
        let mut state = ModelState::default();
        let id = acp::ModelId::new(Arc::from("reasoning-x"));
        let info = acp::ModelInfo::new(id.clone(), "Reasoning X").meta(
            serde_json::json!({ "supportsReasoningEffort": true, "reasoningEffort": "high" })
                .as_object()
                .cloned(),
        );
        state.available.insert(id, info);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        // The preselection must name a row `suggest_args` actually builds, or the consumers fall back to row 0
        let high_row = cmd
            .suggest_args(&ctx, "Reasoning X ")
            .and_then(|items| items.get(1).map(|item| item.insert_text.clone()));
        assert_eq!(Some("Reasoning X high".to_owned()), high_row);
        assert_eq!(high_row, cmd.preselected_arg(&ctx, "Reasoning X "));
        assert_eq!(None, cmd.preselected_arg(&ctx, "Reasoning X h"));
        assert_eq!(None, cmd.preselected_arg(&ctx, ""));
    }

    #[test]
    fn partial_effort_query_still_in_effort_phase() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id, info);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        // Still in effort phase; the matcher upstream narrows to high and xhigh
        let items = cmd.suggest_args(&ctx, "Reasoning X h").unwrap();
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn partial_model_query_stays_in_model_phase() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id, info);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        // No trailing space: the user is still typing the model name
        let items = cmd.suggest_args(&ctx, "Reason").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items.first().map(|item| item.insert_text.as_str()),
            Some("Reasoning X ")
        );
    }

    #[test]
    fn run_parses_model_plus_effort_when_supported() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id, info);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "Reasoning X xhigh");
        match result {
            CommandResult::Action(Action::SwitchModel(ModelChoice {
                model_id, effort, ..
            })) => {
                assert_eq!(model_id.0.as_ref(), "reasoning-x");
                assert_eq!(effort, Some(ReasoningEffort::Xhigh));
            }
            other => panic!("expected SwitchModel with effort, got {other:?}"),
        }
    }

    #[test]
    fn run_rejects_unoffered_effort_with_effort_error_not_unknown_model() {
        // Regression: previously `resolve_effort_token_for` returned None and the handler fell through to `Unknown model: Reasoning X none`
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id, info);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "Reasoning X none");
        match result {
            CommandResult::Error(msg) => {
                assert!(
                    msg.contains("unknown effort level 'none'"),
                    "expected effort error, got {msg}"
                );
                assert!(
                    msg.contains("use one of:"),
                    "expected offered levels in message, got {msg}"
                );
                assert!(
                    !msg.to_lowercase().contains("unknown model"),
                    "must not misreport as unknown model: {msg}"
                );
                let offered = msg.split_once("; ").map(|(_, r)| r).unwrap_or("");
                assert!(
                    !offered.contains("none"),
                    "must not list none as offered: {msg}"
                );
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn run_prefers_full_multi_word_model_name_over_prefix_plus_effort() {
        // The catalog has both "Grok" (reasoning) and "Grok 4.5"
        // `/model Grok 4.5` must select the full name, not treat "4.5" as an effort on "Grok"
        let mut state = ModelState::default();
        let (short_id, short_info) = model_with_reasoning("grok", "Grok");
        let (long_id, long_info) = model_with_reasoning("grok-4.5", "Grok 4.5");
        state.available.insert(short_id, short_info);
        state.available.insert(long_id.clone(), long_info);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "Grok 4.5");
        match result {
            CommandResult::Action(Action::SetDefaultModel(resolved_id)) => {
                assert_eq!(resolved_id, long_id);
            }
            other => panic!("expected SetDefaultModel(Grok 4.5), got {other:?}"),
        }
    }

    #[test]
    fn run_rejects_effort_for_non_reasoning_model() {
        let mut state = ModelState::default();
        let (id, info) = plain_model("grok-4.5", "Grok 4.5");
        state.available.insert(id, info);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "Grok 4.5 high");
        // Falls through to "is the whole string a model name?", which it isn't, so we get an Unknown error
        assert!(matches!(result, CommandResult::Error(_)));
    }

    /// The bare `/model <name>` form dispatches `Action::SetDefaultModel(<ModelId>)` instead of the legacy `Action::SwitchModel { effort: None }`.
    /// The dispatcher routes it through both `Effect::SwitchModel` (session mutation) and `Effect::PersistSetting` (next-session default).
    /// The payload is the typed `acp::ModelId` (resolved at the slash boundary), not a String.
    #[test]
    fn run_bare_model_name_dispatches_set_default_model() {
        let mut state = ModelState::default();
        let (id, info) = plain_model("grok-4.5", "Grok 4.5");
        state.available.insert(id.clone(), info);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "Grok 4.5");
        match result {
            CommandResult::Action(Action::SetDefaultModel(resolved_id)) => {
                assert_eq!(resolved_id, id);
            }
            other => panic!("expected Action::SetDefaultModel(<id>), got {other:?}"),
        }
    }

    /// Case-insensitive matching against the catalog: `/model grok 4.5` resolves to the same `ModelId` as `/model Grok 4.5`.
    #[test]
    fn run_set_default_model_resolves_case_insensitively() {
        let mut state = ModelState::default();
        let (id, info) = plain_model("grok-4.5", "Grok 4.5");
        state.available.insert(id.clone(), info);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "grok 4.5");
        match result {
            CommandResult::Action(Action::SetDefaultModel(resolved_id)) => {
                assert_eq!(resolved_id, id);
            }
            other => panic!("expected Action::SetDefaultModel(<id>), got {other:?}"),
        }
    }

    // ── Locked platform models (BYOK discovery) ─────────────────────

    fn locked_platform_model(id: &str, name: &str) -> (acp::ModelId, acp::ModelInfo) {
        let id = acp::ModelId::new(Arc::from(id));
        let meta = serde_json::json!({
            "supportsReasoningEffort": true,
            "requiresApiKey": true,
            "platform": "deepseek",
            "platformName": "DeepSeek",
            "apiKeyEnv": ["GROK_DEEPSEEK_API_KEY", "DEEPSEEK_API_KEY"],
            "setupHint": "export GROK_DEEPSEEK_API_KEY=<key> (or DEEPSEEK_API_KEY), or add `api_key = \"<key>\"` under `[platforms.deepseek]` in ~/.grok/config.toml",
        })
        .as_object()
        .cloned()
        .unwrap();
        let info = acp::ModelInfo::new(id.clone(), name.to_string()).meta(Some(meta));
        (id, info)
    }

    #[test]
    fn locked_models_sort_last_with_lock_marker_and_no_effort_chain() {
        let mut state = ModelState::default();
        let (lid, linfo) = locked_platform_model("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash");
        let (pid, pinfo) = plain_model("grok-4.5", "Grok 4.5");
        state.available.insert(lid, linfo);
        state.available.insert(pid, pinfo);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        let items = cmd.suggest_args(&ctx, "").unwrap();
        assert_eq!(items.len(), 2);
        // Usable model first, locked row last.
        assert_eq!(items[0].match_text, "Grok 4.5");
        assert!(!items[0].locked);
        let locked = &items[1];
        assert!(locked.locked, "locked row must carry the flag for dimming");
        assert!(locked.display.starts_with('🔒'), "lock marker: {locked:?}");
        assert!(locked.display.contains("DeepSeek"), "provider in display");
        // Managed-platform rows insert the catalog id (unique across providers).
        // No trailing space — Enter must submit (→ lock error), never chain
        // into an effort sub-menu, even though the model supports effort.
        assert_eq!(locked.insert_text, "deepseek/deepseek-v4-flash");
        assert!(locked.description.contains("DEEPSEEK_API_KEY"));
    }

    #[test]
    fn locked_reasoning_model_trailing_space_stays_in_model_phase() {
        let mut state = ModelState::default();
        let (lid, linfo) = locked_platform_model("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash");
        state.available.insert(lid, linfo);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        // Trailing space after a locked reasoning-capable model must NOT
        // enter the effort phase — the model list is re-rendered instead.
        let items = cmd.suggest_args(&ctx, "DeepSeek V4 Flash ").unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].locked);
    }

    #[test]
    fn run_locked_model_exact_match_returns_setup_error() {
        let mut state = ModelState::default();
        let (lid, linfo) = locked_platform_model("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash");
        state.available.insert(lid, linfo);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "DeepSeek V4 Flash");
        match result {
            CommandResult::Error(msg) => {
                assert!(msg.contains("DeepSeek"), "provider named: {msg}");
                assert!(msg.contains("DEEPSEEK_API_KEY"), "env var in hint: {msg}");
                assert!(msg.contains("/providers"), "points at /providers: {msg}");
            }
            other => panic!("expected lock Error, got {other:?}"),
        }
    }

    #[test]
    fn run_locked_model_with_effort_token_returns_setup_error() {
        let mut state = ModelState::default();
        let (lid, linfo) = locked_platform_model("deepseek/deepseek-v4-flash", "DeepSeek V4 Flash");
        state.available.insert(lid, linfo);
        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "DeepSeek V4 Flash high");
        match result {
            CommandResult::Error(msg) => {
                assert!(msg.contains("DEEPSEEK_API_KEY"), "env var in hint: {msg}");
            }
            other => panic!("expected lock Error, got {other:?}"),
        }
    }

    // ── Multi-provider same display name ─────────────────────────────

    fn usable_managed_model(
        id: &str,
        name: &str,
        description: &str,
    ) -> (acp::ModelId, acp::ModelInfo) {
        let id = acp::ModelId::new(Arc::from(id));
        let mut meta = serde_json::Map::new();
        meta.insert(
            "supportsReasoningEffort".into(),
            serde_json::Value::Bool(true),
        );
        let info = acp::ModelInfo::new(id.clone(), name.to_string())
            .description(Some(description.to_string()))
            .meta(serde_json::Value::Object(meta).as_object().cloned());
        (id, info)
    }

    #[test]
    fn multi_provider_same_name_shows_provider_and_unique_insert() {
        let mut state = ModelState::default();
        let (a_id, a_info) = usable_managed_model(
            "zai-coding-cn/glm-5.2",
            "GLM-5.2",
            "Official Pi catalog (zai-coding-cn)",
        );
        let (b_id, b_info) =
            usable_managed_model("ollama/glm-5.2", "GLM-5.2", "Official Pi catalog (ollama)");
        state.available.insert(a_id, a_info);
        state.available.insert(b_id, b_info);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        let items = cmd.suggest_args(&ctx, "").unwrap();
        assert_eq!(items.len(), 2);

        let zai = items
            .iter()
            .find(|i| i.action_id.as_deref() == Some("zai-coding-cn/glm-5.2"))
            .expect("zai row");
        let ollama = items
            .iter()
            .find(|i| i.action_id.as_deref() == Some("ollama/glm-5.2"))
            .expect("ollama row");

        // Left column disambiguates; right column is the human provider name
        // (not the useless "Official Pi catalog (...)" stamp).
        assert!(
            zai.display.contains("Z.AI Coding Plan (CN)"),
            "display: {}",
            zai.display
        );
        assert_eq!(zai.description, "Z.AI Coding Plan (CN)");
        assert_eq!(zai.insert_text, "zai-coding-cn/glm-5.2 ");
        assert!(
            ollama.display.contains("Ollama Cloud"),
            "display: {}",
            ollama.display
        );
        assert_eq!(ollama.description, "Ollama Cloud");
        assert_eq!(ollama.insert_text, "ollama/glm-5.2 ");

        // Type-to-filter can find by provider name.
        assert!(zai.match_text.to_lowercase().contains("coding"));
        assert!(ollama.match_text.to_lowercase().contains("ollama"));
    }

    #[test]
    fn multi_provider_effort_phase_uses_catalog_id_token() {
        let mut state = ModelState::default();
        let (a_id, a_info) = usable_managed_model(
            "zai-coding-cn/glm-5.2",
            "GLM-5.2",
            "Official Pi catalog (zai-coding-cn)",
        );
        let (b_id, b_info) =
            usable_managed_model("ollama/glm-5.2", "GLM-5.2", "Official Pi catalog (ollama)");
        state.available.insert(a_id, a_info);
        state.available.insert(b_id, b_info);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        // Ambiguous bare name + space must NOT enter effort phase.
        let items = cmd.suggest_args(&ctx, "GLM-5.2 ").unwrap();
        assert!(
            items.iter().all(|i| i.action_id.is_some()),
            "must stay in model phase for ambiguous name"
        );

        // Provider-scoped id + space enters effort with that id as the token.
        let items = cmd.suggest_args(&ctx, "zai-coding-cn/glm-5.2 ").unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].insert_text, "zai-coding-cn/glm-5.2 xhigh");
        assert_eq!(items[1].insert_text, "zai-coding-cn/glm-5.2 high");
    }

    #[test]
    fn run_ambiguous_display_name_lists_providers() {
        let mut state = ModelState::default();
        let (a_id, a_info) = usable_managed_model(
            "zai-coding-cn/glm-5.2",
            "GLM-5.2",
            "Official Pi catalog (zai-coding-cn)",
        );
        let (b_id, b_info) =
            usable_managed_model("ollama/glm-5.2", "GLM-5.2", "Official Pi catalog (ollama)");
        state.available.insert(a_id.clone(), a_info);
        state.available.insert(b_id, b_info);

        let mut ctx = dummy_exec_ctx(&state);
        let result = ModelCommand.run(&mut ctx, "GLM-5.2");
        match result {
            CommandResult::Error(msg) => {
                assert!(msg.contains("multiple providers"), "msg: {msg}");
                assert!(msg.contains("zai-coding-cn/glm-5.2"), "msg: {msg}");
                assert!(msg.contains("ollama/glm-5.2"), "msg: {msg}");
            }
            other => panic!("expected ambiguous Error, got {other:?}"),
        }

        // Explicit catalog id still works.
        let result = ModelCommand.run(&mut ctx, "zai-coding-cn/glm-5.2");
        match result {
            CommandResult::Action(Action::SetDefaultModel(id)) => {
                assert_eq!(id, a_id);
            }
            other => panic!("expected SetDefaultModel, got {other:?}"),
        }
    }

    #[test]
    fn unique_managed_model_shows_provider_description_uses_catalog_id_insert() {
        // Only one GLM-5.2: left stays the friendly name; right shows provider;
        // insert uses catalog id (managed rows always do — stable if another
        // provider later ships the same display name).
        let mut state = ModelState::default();
        let (id, info) = usable_managed_model(
            "zai-coding-cn/glm-5.2",
            "GLM-5.2",
            "Official Pi catalog (zai-coding-cn)",
        );
        state.available.insert(id, info);

        let cmd = ModelCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            saved_workflows: &[],
            workflow_runs: &[],
            screen_mode: crate::app::ScreenMode::Fullscreen,
            current_title: None,
        };
        let items = cmd.suggest_args(&ctx, "").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].display, "GLM-5.2");
        assert_eq!(items[0].description, "Z.AI Coding Plan (CN)");
        assert_eq!(items[0].insert_text, "zai-coding-cn/glm-5.2 ");
    }
}
