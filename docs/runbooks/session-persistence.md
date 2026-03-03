# Session Persistence Verification

**Requirement:** DEBT-05
**Type:** Human test
**Duration:** ~15 minutes

## Prerequisites
- Blufio running with gateway enabled
- curl or similar HTTP client available
- Database path known from config

## Steps

1. Send a message via gateway:
   ```bash
   curl -X POST http://localhost:3000/v1/messages \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"content": "Remember the codeword: swordfish"}'
   ```
2. Note the session_id from the response
3. Verify session exists:
   ```bash
   curl -H "Authorization: Bearer <token>" \
     http://localhost:3000/v1/sessions
   ```
4. Stop Blufio gracefully: `systemctl stop blufio` or Ctrl+C
5. Verify process exited: `pgrep blufio` returns nothing
6. Start Blufio again: `systemctl start blufio` or `blufio serve`
7. Check sessions after restart:
   ```bash
   curl -H "Authorization: Bearer <token>" \
     http://localhost:3000/v1/sessions
   ```
8. Verify: Previous session data visible in the response
9. Verify: Database file preserved (check file modification time)

## Pass Criteria
- [ ] Session data survives process restart
- [ ] GET /v1/sessions shows previous session after restart
- [ ] Database file intact after restart
- [ ] No data corruption in logs

## Failure Actions
- Check database path in config
- Verify WAL mode: `sqlite3 <db_path> "PRAGMA journal_mode"`
- Check file permissions on database directory
