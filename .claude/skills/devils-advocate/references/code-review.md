# Code Review — Devil's Advocate Checklist

## Failure Modes to Hunt

### Architecture & Design
- What happens when this component goes down? Is there a fallback?
- Are there circular dependencies or tight coupling hiding here?
- Does this scale? What breaks at 10x, 100x, 1000x current load?
- Is there a single point of failure?
- What's the migration path when requirements inevitably change?

### Edge Cases & Input
- What happens with empty input? Null? Undefined? NaN?
- Maximum length strings, zero-length arrays, negative numbers
- Unicode, emoji, RTL text, special characters
- Concurrent access — race conditions, deadlocks
- What if the network is slow, flaky, or completely down?
- What if the database is full, locked, or corrupted?
- What if the clock is wrong or timezone changes?

### Security
- Injection vectors: SQL, XSS, command injection, path traversal
- Auth bypass: Can unauthenticated users reach this? Can users escalate privileges?
- Data exposure: Are secrets in logs? Are error messages too verbose?
- CSRF, SSRF, open redirects
- Dependency vulnerabilities — when was the last audit?

### Error Handling
- Are errors caught at the right level?
- Do error messages leak internal details?
- Is there a global error boundary? What happens when it fails?
- Are retries bounded? Can they cascade into a storm?
- What does the user see when things break?

### Performance
- N+1 queries? Unbounded loops? Memory leaks?
- What's cached? What happens on cache miss? Cache stampede?
- Are there blocking calls in async paths?
- What's the worst-case response time?

### Data Integrity
- What happens during partial failures mid-transaction?
- Are writes idempotent? What if the same request fires twice?
- Version migration: what happens to existing data?
- Backup and recovery — tested or theoretical?

## Output Format

Structure findings by severity:

1. **🔴 Critical** — Will break in production or cause data loss
2. **🟠 Serious** — Will cause problems under realistic conditions
3. **🟡 Moderate** — Could cause issues in edge cases
4. **🔵 Minor** — Code smell or improvement opportunity

For each finding:
- **What:** One-line description
- **How it breaks:** Concrete scenario
- **Suggestion:** How to fix or mitigate
