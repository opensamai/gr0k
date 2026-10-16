# Environment variables

Operator table of feature environment variables. The compile-time check requires each variable name in backticks.

| Variable | Feature key | Default |
| --- | --- | --- |
| `GROK_SESSION_SEARCH` | `session_search` | true |
| `GROK_LSP_TOOLS` | `lsp_tools` | false |
| `GROK_WEB_FETCH` | `web_fetch` | false |
| `GROK_SESSION_RECAP` | `session_recap` | true |
| `GROK_ASK_USER_QUESTION` | `ask_user_question` | true |
| `GROK_VOICE_MODE` | `voice_mode` | true |
| `GROK_WRITE_FILE` | `write_file` | true |
| `GROK_FEEDBACK_ENABLED` | `feedback` | true |
| `GROK_FEEDBACK_TRACE_CARD` | `feedback_trace_card` | false |
| `GROK_TURN_SUMMARY` | `turn_summary` | true |
| `GROK_CANCEL_REWIND` | `cancel_rewind` | true |
| `GROK_COMPACTION_VERBATIM_INPUT` | `compaction_verbatim_input` | true |
| `GROK_TWO_PASS_COMPACTION` | `two_pass_compaction` | true |
| `GROK_BACKEND_SEARCH` | `backend_tools` | true |
| `GROK_AUTO_WAKE` | `auto_wake` | true |
| `GROK_SUBAGENT_WORKTREE_SNAPSHOT` | `subagent_worktree_snapshot` | false |
| `GROK_SUBAGENT_MODEL_INHERITANCE` | `subagent_model_inheritance` | false |
| `GROK_ACTIVE_AGENT_MESSAGES` | `active_agent_messages` | false |
| `GROK_DOCK` | `dock` | false |
| `GROK_TERMINAL_THEME` | `terminal_theme` | false |
| `GROK_FILE_ACCELERATION` | `file_acceleration` | false |

These rows mirror `xai_grok_config_types::FEATURES`. A variable set to `1`/`true` or `0`/`false` overrides `config.toml` for that feature.
