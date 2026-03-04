// ── Hook event → state mapping tests ──
//
// Tests determine_state() which maps HookEvent + tool_name to state strings.

use cove_cli::cli::HookEvent;
use cove_cli::commands::hook;

#[test]
fn event_to_state_mapping() {
    // Direct mappings (tool_name doesn't matter for these)
    assert_eq!(hook::determine_state(&HookEvent::UserPrompt, ""), "working");
    assert_eq!(hook::determine_state(&HookEvent::AskDone, ""), "working");
    assert_eq!(hook::determine_state(&HookEvent::PostTool, ""), "working");
    assert_eq!(hook::determine_state(&HookEvent::Stop, ""), "idle");
    assert_eq!(hook::determine_state(&HookEvent::SessionEnd, ""), "end");
    assert_eq!(hook::determine_state(&HookEvent::Ask, ""), "asking");
}

#[test]
fn pre_tool_asking_vs_waiting() {
    // Asking tools → "asking"
    assert_eq!(
        hook::determine_state(&HookEvent::PreTool, "AskUserQuestion"),
        "asking"
    );

    // Non-asking tools → "waiting"
    assert_eq!(
        hook::determine_state(&HookEvent::PreTool, "Bash"),
        "waiting"
    );
    assert_eq!(
        hook::determine_state(&HookEvent::PreTool, "Read"),
        "waiting"
    );
    assert_eq!(
        hook::determine_state(&HookEvent::PreTool, "Write"),
        "waiting"
    );
}

#[test]
fn all_asking_tools() {
    // All tools in ASKING_TOOLS should produce "asking"
    for tool in hook::ASKING_TOOLS {
        assert_eq!(
            hook::determine_state(&HookEvent::PreTool, tool),
            "asking",
            "tool {tool} should map to 'asking'"
        );
    }
    assert_eq!(hook::ASKING_TOOLS.len(), 3);
}
