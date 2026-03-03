# SIGTERM Drain Timing Verification

**Requirement:** DEBT-06
**Type:** Human test
**Duration:** ~15 minutes

## Prerequisites
- Blufio running with gateway enabled
- Ability to send SIGTERM (kill command or systemctl)
- curl available for in-flight request simulation

## Steps

1. Start Blufio: `blufio serve`
2. In terminal A, send a message that will trigger a long LLM response:
   ```bash
   curl -X POST http://localhost:3000/v1/messages \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"content": "Write a detailed 500-word essay about the history of computing"}'
   ```
3. While the request is in-flight (before response), in terminal B:
   ```bash
   kill -SIGTERM $(pgrep blufio)
   ```
4. Observe terminal A:
   - Verify: The in-flight request completes (receives a response)
   - Note the time between SIGTERM and process exit
5. Check logs for graceful shutdown messages:
   ```bash
   journalctl -u blufio --since "2 minutes ago" | grep -i "shutdown\|drain\|signal"
   ```
6. Verify: Process exits within 60 seconds of SIGTERM
7. Verify: No "connection reset" or truncated response in terminal A

## Pass Criteria
- [ ] In-flight request completes after SIGTERM
- [ ] Process exits within 60 seconds
- [ ] Logs show graceful shutdown sequence
- [ ] No connection resets or truncated responses
- [ ] Subsequent requests after SIGTERM are rejected (503 or connection refused)

## Failure Actions
- Check signal handler: grep for install_signal_handler in serve.rs
- Verify TimeoutStopSec in systemd unit file
- Check for blocking operations that prevent shutdown
