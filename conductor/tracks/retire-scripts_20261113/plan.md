# Plan — retire-scripts (gluon TD-46)

## Phase 1: Acceptance and coverage map
- [ ] Task: `scripts/verify-retire-scripts.sh`, red
  - [ ] Checks: the 12 gone; kept 5 present + `bash -n` clean; no `git grep` hits outside `conductor/archive`; `contract_checks` gone from `scripts/lib/verify.sh`; `workflow.md` states rule; `scripts/ci.sh` green
  - [ ] Run: red for stated reasons (scripts present, rule absent)
- [ ] Task: Coverage map in this plan — each section of the 12 → named covering test (Rust test / ci.sh step / harness check)
  - [ ] Any section unmapped → Rust test written first (red on mutation, then green)
- [ ] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)

## Phase 2: Retire
- [ ] Task: Delete the 12 `scripts/verify-*.sh`; drop `contract_checks` from `scripts/lib/verify.sh`
- [ ] Task: Live references updated (README, conductor product/tech-stack); archives untouched
- [ ] Task: Conductor - User Manual Verification 'Phase 2' (Protocol in workflow.md)

## Phase 3: Rule
- [ ] Task: `conductor/workflow.md` Phase Completion step 4 — retirement rule: at archive, script sections covered by CI/harness → script deleted; CI-runnable remainder → moved to CI; only non-CI-runnable kept
- [ ] Task: `verify-retire-scripts.sh` PASS; failure path proven on a copy
- [ ] Task: Conductor - User Manual Verification 'Phase 3' (Protocol in workflow.md)

At archive: `verify-retire-scripts.sh` itself retired (per the rule). gluon follow-up (verify-port lists, docs table, TD-46 tick) recorded, done in gluon root.
