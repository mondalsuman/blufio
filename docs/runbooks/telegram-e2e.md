# Telegram E2E Verification

**Requirement:** DEBT-04
**Type:** Human test
**Duration:** ~10 minutes

## Prerequisites
- Blufio running with Telegram bot token configured
- Telegram account with access to the bot
- `blufio doctor` shows all checks passing

## Steps

1. Open Telegram and navigate to the Blufio bot
2. Send a simple text message: "Hello, what can you do?"
3. Verify: Response received within 30 seconds
4. Verify: Response is coherent and contextually appropriate
5. Send a follow-up: "Remember the number 42"
6. Send: "What number did I just tell you?"
7. Verify: Agent references 42 (conversation context preserved)
8. Check `blufio doctor` -- verify no new errors

## Pass Criteria
- [ ] Bot responds to messages within 30s
- [ ] Responses are contextually appropriate
- [ ] Conversation context maintained within session
- [ ] No errors in agent logs during test

## Failure Actions
- Check logs: `journalctl -u blufio -f`
- Verify bot token: `blufio doctor`
- Check Telegram API connectivity
