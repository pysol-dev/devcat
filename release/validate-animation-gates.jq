# Usage: jq -e -f release/validate-animation-gates.jq release/animation-gates.json
def nonempty_string: type == "string" and test("\\S");
def evidence_record:
  type == "object"
  and (.wrapper_commit | type == "string" and test("^[0-9a-f]{40}$"))
  and (.artwork_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and ([.child_executable, .child_version, .os, .terminal,
        .terminal_version, .renderer, .exact_test, .screenshot,
        .recording, .cleanup] | all(.[]; nonempty_string))
  and (.commands | type == "array" and length > 0 and all(.[]; nonempty_string))
  and .result == "pass";
def gate($version):
  type == "object" and .version == $version
  and (.status == "pending" or .status == "blocked" or .status == "pass")
  and (.evidence | type == "array")
  and (if .status == "pass" then
         (.evidence | length > 0 and all(.[]; evidence_record))
       else .evidence == [] end)
  and (if .status == "blocked" then (.blocker | nonempty_string) else true end);
def valid:
  type == "object" and .schema_version == 1
  and (.utilities_unlocked | type == "boolean")
  and (.gates | type == "object" and
    (keys == ["controlled_foundation", "shared_compatibility", "stock_claude", "stock_codex", "stock_hermes"]))
  and (.gates.controlled_foundation | gate("0.0.1"))
  and (.gates.stock_hermes | gate("0.1.0"))
  and (.gates.stock_claude | gate("0.2.0"))
  and (.gates.stock_codex | gate("0.3.0"))
  and (.gates.shared_compatibility | gate("0.4.0"))
  and ([.gates.controlled_foundation, .gates.stock_hermes, .gates.stock_claude,
        .gates.stock_codex, .gates.shared_compatibility] |
        [.[].status == "pass"] as $passed |
        all(range(1; length); . as $i | ($passed[$i] | not) or $passed[$i - 1]))
  and (if .utilities_unlocked then all(.gates[]; .status == "pass") else true end);
if valid then true else error("Invalid animation gates: utilities require every gate to pass in order with complete evidence") end
