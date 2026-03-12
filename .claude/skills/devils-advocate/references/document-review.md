# Document & PRD Review — Devil's Advocate Checklist

## Structure & Clarity
- Can someone understand the purpose in 30 seconds?
- Are there ambiguous requirements? ("fast", "scalable", "user-friendly" — what do these MEAN?)
- Are success metrics defined and measurable?
- Is scope clearly bounded? What's explicitly OUT of scope?

## Completeness
- Error states and unhappy paths documented?
- Edge cases addressed or acknowledged?
- Dependencies listed with fallback plans?
- Migration path from current state?
- Rollback plan if things go wrong?
- Legal/compliance considerations? (GDPR, accessibility, licensing)

## Consistency
- Do sections contradict each other?
- Are terms used consistently? (Watch for same concept, different names)
- Do timelines match across sections?
- Do resource estimates match the scope described?

## Realism
- Are timelines realistic or aspirational?
- Are resource requirements accounted for?
- Are risks acknowledged or hand-waved?
- Is the "MVP" actually minimal?

## Missing Perspectives
- User onboarding experience — what's the first 5 minutes?
- Abuse/misuse vectors — how can bad actors exploit this?
- Operational concerns — monitoring, alerting, on-call burden
- Data lifecycle — retention, deletion, privacy

## Output Format

1. **Document Health** — Overall quality score (Solid / Needs Work / Major Gaps)
2. **Ambiguities** — Vague requirements that need clarification
3. **Contradictions** — Places where the doc disagrees with itself
4. **Blind Spots** — What's missing entirely
5. **Nitpicks** — Small improvements that add polish
6. **What's Good** — Sections that are well-written and thorough
