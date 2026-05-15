# AI Disclosure

## 1. Summary

I used **Claude Code** (Anthropic's CLI-based AI coding assistant) throughout this project across all phases: planning, implementation, code review, testing analysis, edge case analysis. The AI acted as a pair programmer and reviewer. I defined the requirements from the PDF specification, made all architectural decisions, reviewed and corrected AI-generated code, and determined which suggestions to apply or reject. Every line of code in this submission was reviewed and approved by me before being committed.

## 2. AI Usage by Phase

### Planning
- **What AI did:** Structured an 8-task implementation plan with TDD approach based on the PDF spec requirements I provided.
- **What I contributed:** Defined all requirements from the specification, prioritized tasks, and validated the plan scope.

### Implementation
- **What AI did:** Implemented each task sequentially, writing tests first then implementation code.
- **What I contributed:** Reviewed every task upon completion, corrected bugs (wrong test assertions, lifetime errors, incorrect chargeback logic), and approved or rejected changes before moving on.

### Code Review
- **What AI did:** Ran spec compliance checks and code quality reviews after each task.
- **What I contributed:** Evaluated each finding, decided which fixes were warranted, and applied or dismissed them based on project priorities.

### Rust Patterns Review
- **What AI did:** Reviewed code against idioms from rust-unofficial.github.io/patterns/. Suggested removing unused `Clone` derives and fixing a clippy warning.
- **What I contributed:** Assessed each suggestion against the codebase needs and applied the ones that improved quality without over-engineering.

### Testing Analysis
- **What AI did:** Analyzed the test suite against principles from "Effective Software Testing" by Mauricio Aniche. Identified gaps in coverage (negative amounts, duplicate transaction IDs, guard clause tests).
- **What I contributed:** Prioritized findings by impact, implemented the fixes myself, and added the missing test cases.

### Architecture Review
- **What AI did:** Identified a design tension in the withdrawal chargeback invariant (a withdrawal followed by a dispute can push the client balance below zero).
- **What I contributed:** Analyzed the spec, decided this is an inherent edge case in the specification's own rules, and chose to document it rather than silently change the behavior.

### Edge Case Analysis
- **What AI did:** A PM-mode agent found 5 red-flag issues including blank lines in CSV, >4dp precision handling, BOM characters, missing headers, and the withdrawal chargeback invariant.
- **What I contributed:** Assessed each issue, fixed 2 real bugs (blank line parsing, >4dp precision), accepted BOM/header-less CSV as acceptable behavior, and documented the invariant tension.

## 3. Key Decisions Made by me (Not AI)

1. **Resolved the spec contradiction for withdrawal disputes.** The spec says "total should remain the same" during disputes, but disputing a withdrawal requires changing total to maintain the `total == available + held` invariant. I chose to change total during withdrawal disputes (keeping the invariant intact during the dispute phase) and document the resulting invariant break after withdrawal chargebacks.

2. **Corrected AI's wrong test assertions.** The AI generated `chargeback_freezes_account` tests with expected balance values that didn't match the spec. I caught this during review and corrected the assertions.

3. **Chose to document the withdrawal chargeback invariant break.** The AI identified the tension. I decided documenting it transparently was better than adding special-case logic that would deviate from the spec.

4. **Decided which edge cases to fix vs accept.** Fixed blank-line CSV parsing and >4dp precision truncation. Accepted BOM and header-less CSV as non-issues since the spec defines a fixed format.

5. **Structured the project with 3-module separation** (`models`, `engine`, `main`) for clean separation of concerns.

6. **Used `HashMap` over `BTreeMap`** for O(1) client lookups, since the spec does not require sorted output.

7. **Used `rust_decimal` over `f64`** for financial precision, avoiding floating-point rounding errors.

## 4. Tools

- **Claude Code** (Anthropic) -- CLI-based AI coding assistant used throughout all phases
- **Skills used:**
  - `superpowers:writing-plans` -- structured the implementation plan
  - `superpowers:subagent-driven-development` -- parallel task execution
  - `effective-software-testing` -- test suite quality analysis
  - `rust-patterns-skill` -- Rust idioms and patterns review
