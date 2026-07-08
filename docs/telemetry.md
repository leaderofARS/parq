# AI-Native Machine Telemetry

`parq` is designed for agentic coding workflows. Traditional dataframe tools throw dense stack traces containing nested Python and C++ lines. These traces are hard for AI coding assistants (like Cursor, Windsurf, or custom LLM orchestration loops) to parse and debug autonomously.

`parq` features a `--machine-telemetry` flag that converts pipeline ingestion errors into strict, machine-readable JSON outputs on `stderr`, allowing autonomous agents to diagnose and resolve ingestion script bugs without human intervention.

---

## 1. Enabling Telemetry

To enable telemetry output, pass the `--machine-telemetry` flag:

```bash
parq -i dirty.jsonl -o output.parquet --machine-telemetry
```

When this flag is active:
* All standard logging (via `tracing`) to `stdout`/`stderr` is suppressed.
* On success, a structured JSON confirmation is printed to `stdout` with exit code `0`.
* On failure, a structured JSON error payload is printed to `stderr` with exit code `1`.

---

## 2. Telemetry Schema Specifications

### On Success
```json
{
  "status": "success",
  "rows_processed": 5000000
}
```

### On JSON Parse Failures
Triggered when a line in the NDJSON contains corrupt characters, trailing commas, or incomplete brackets:
```json
{
  "status": "failed",
  "error_type": "JsonParse",
  "details": {
    "line": 4820,
    "message": "expected value at line 1 column 12"
  }
}
```

### On Type Mismatches
Triggered when a value in the input does not match the inferred or explicitly defined Arrow column type (e.g. attempting to parse a string into a boolean column):
```json
{
  "status": "failed",
  "error_type": "TypeMismatch",
  "details": {
    "field": "active",
    "expected": "Boolean",
    "found": "Number(123)",
    "line": 2
  }
}
```

### On Insufficient Data
Triggered when the file is empty or contains only whitespace:
```json
{
  "status": "failed",
  "error_type": "InsufficientData",
  "details": {
    "rows": 0
  }
}
```

### On Generic Runtime Errors
Triggered on missing files, permission issues, or general operating system errors:
```json
{
  "status": "failed",
  "error_type": "Generic",
  "message": "No such file or directory (os error 2)"
}
```

---

## 3. Automated AI Debugging Example

When an orchestration agent executes `parq` and captures a telemetry error, it can handle the failure programmatically:

```python
import json
import subprocess

# Run parq with telemetry enabled
res = subprocess.run([
    "./parq", "-i", "dataset.jsonl", "-o", "out.parquet", "--machine-telemetry"
], capture_output=True, text=True)

if res.returncode != 0:
    error_payload = json.loads(res.stderr)
    print(f"Agent detected error: {error_payload['error_type']}")
    
    if error_payload['error_type'] == "TypeMismatch":
        details = error_payload['details']
        # The agent can autonomously rewrite the schema config, 
        # change active to Int64, or enable --ignore-errors
        print(f"Action: Rewriting schema to match expected type on line {details['line']}")
```
