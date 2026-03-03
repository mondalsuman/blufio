# Memory Bounds 72-Hour Verification

**Requirement:** DEBT-07
**Type:** Human test (long-running)
**Duration:** 72+ hours

## Prerequisites
- Blufio running on a stable server (VPS or local machine that won't sleep)
- Prometheus metrics enabled and scraping configured (or manual monitoring)
- `blufio doctor --deep` passing

## Setup

1. Start Blufio with memory monitoring enabled:
   ```bash
   RUST_LOG=blufio=info blufio serve
   ```
2. Record baseline memory from doctor:
   ```bash
   blufio doctor --deep
   # Note: Memory baseline -- heap: X MB, resident: Y MB
   ```
3. If Prometheus is configured, set up a dashboard or recording rule for:
   - `blufio_memory_heap_bytes`
   - `blufio_memory_resident_bytes`
   - `blufio_memory_rss_bytes`
   - `blufio_memory_pressure`

## During Test (72 hours)

4. Periodically send messages (every ~30 minutes or via heartbeat):
   ```bash
   # Manual check script (run via cron every 30 min)
   curl -s -X POST http://localhost:3000/v1/messages \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"content": "Status check"}' > /dev/null
   echo "$(date): $(blufio doctor --deep --plain 2>&1 | grep 'Memory baseline')"
   ```
5. At 24h, 48h, and 72h checkpoints, run:
   ```bash
   blufio doctor --deep
   ```
   Record memory values.

## After 72 Hours

6. Run final doctor check:
   ```bash
   blufio doctor --deep
   ```
7. Compare final heap/resident/RSS with baseline
8. Check for memory pressure warnings in logs:
   ```bash
   journalctl -u blufio --since "72 hours ago" | grep -c "memory pressure"
   ```

## Pass Criteria
- [ ] Heap memory growth < 50% over baseline after 72h
- [ ] No OOM kills
- [ ] blufio_memory_pressure never sustained at 1.0 for > 5 minutes
- [ ] Process still responsive at 72h mark
- [ ] No memory-related errors in logs

## Failure Actions
- Check for memory leaks: compare heap vs resident divergence
- Review jemalloc stats for arena fragmentation
- Check if memory growth correlates with session count
- Consider reducing memory_warn_mb threshold
