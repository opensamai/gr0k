# Enterprise feature pins

Operator table of registered feature keys. The compile-time check in `registered_features_are_documented` requires each key in backticks.

| Key | Config path | Default |
| --- | --- | --- |
| `session_search` | `features.session_search` | true |
| `lsp_tools` | `features.lsp_tools` | false |
| `web_fetch` | `features.web_fetch` | false |
| `session_recap` | `features.session_recap` | true |
| `ask_user_question` | `features.ask_user_question` | true |
| `voice_mode` | `features.voice_mode` | true |
| `write_file` | `features.write_file` | true |
| `feedback` | `features.feedback` | true |
| `feedback_trace_card` | `features.feedback_trace_card` | false |
| `turn_summary` | `features.turn_summary` | true |
| `cancel_rewind` | `features.cancel_rewind` | true |
| `compaction_verbatim_input` | `features.compaction_verbatim_input` | true |
| `two_pass_compaction` | `features.two_pass_compaction` | true |
| `backend_tools` | `features.backend_tools` | true |
| `auto_wake` | `features.auto_wake` | true |
| `subagent_worktree_snapshot` | `features.subagent_worktree_snapshot` | false |
| `subagent_model_inheritance` | `features.subagent_model_inheritance` | false |
| `active_agent_messages` | `features.active_agent_messages` | false |
| `dock` | `features.dock` | false |
| `terminal_theme` | `features.terminal_theme` | false |
| `file_acceleration` | `features.file_acceleration` | false |

These rows mirror `xai_grok_config_types::FEATURES`. They are the public stand-in for the monorepo-only internal pinning doc the test was written against.
