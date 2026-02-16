const OBSERVER_EXTRACTION_INSTRUCTIONS: &str = r"CRITICAL: DISTINGUISH USER ASSERTIONS FROM QUESTIONS

When the user TELLS you something about themselves, mark it as an assertion.
When the user ASKS about something, mark it as a question/request.
User assertions are authoritative and should be preserved for future retrieval.

STATE CHANGES AND UPDATES:
When new information supersedes prior information, make the state change explicit.
If the new state contradicts older observations, preserve only the current state.

TEMPORAL ANCHORING:
Preserve temporal context and convert relative references to estimated dates when possible.
Keep the statement time and the referenced time distinct.

PRESERVE DETAIL DENSITY:
- Names, handles, identifiers
- Numbers, counts, quantities, units
- Dates, times, durations, ordering/sequence
- Locations and distinguishing attributes
- User preferences and constraints
- Technical results and concrete metrics
- Relevant code and tool outcomes

PRESERVE UNUSUAL PHRASING:
When the user uses special wording, preserve exact wording where it affects meaning.";

const OBSERVER_OUTPUT_FORMAT_BASE: &str = r#"Use priority levels:
- 🔴 High: explicit user facts, preferences, goals achieved, critical context
- 🟡 Medium: project details, learned information, tool results
- 🟢 Low: minor details, uncertain observations

Group related observations by date and list each with 24-hour time.

<observations>
Date: Dec 4, 2025
* 🔴 (14:30) User prefers direct answers
* 🟡 (14:31) Working on feature X
</observations>

<current-task>
Primary: What the agent is currently working on
Secondary: Pending tasks (mark as "waiting for user" when blocked)
</current-task>

<suggested-response>
Hint for the agent's immediate next message.
</suggested-response>"#;

const OBSERVER_GUIDELINES: &str = r"- Be specific enough for future action.
- Use dense language; avoid repetition.
- Do not repeat previously observed facts.
- Preserve tool actions, outcomes, and why they were run.
- Preserve line-level references when code context matters.
- Capture what happened and what it implies.";

#[must_use]
pub fn build_observer_system_prompt() -> String {
    format!(
        r"You are the memory consciousness of an AI assistant. Your observations will be the ONLY information the assistant has about past interactions with this user.

Extract observations that will help the assistant remember:

{OBSERVER_EXTRACTION_INSTRUCTIONS}

=== OUTPUT FORMAT ===

Your output MUST use XML tags to structure the response:

{OBSERVER_OUTPUT_FORMAT_BASE}

=== GUIDELINES ===

{OBSERVER_GUIDELINES}

=== IMPORTANT: THREAD ATTRIBUTION ===

Do NOT add thread identifiers, thread IDs, or <thread> tags in this mode.
Thread attribution is handled by the system.

Remember: these observations are the assistant's only memory.

User messages are extremely important. If the user asks a question or gives a new task, make it clear in <current-task> that this is the priority. If the assistant needs to respond to the user, indicate in <suggested-response> that it should pause for user reply before continuing other tasks."
    )
}

#[must_use]
pub fn build_multi_thread_observer_system_prompt() -> String {
    format!(
        r#"You are the memory consciousness of an AI assistant. Your observations will be the ONLY information the assistant has about past interactions with this user.

Extract observations that will help the assistant remember:

{OBSERVER_EXTRACTION_INSTRUCTIONS}

=== MULTI-THREAD INPUT ===

You will receive messages from MULTIPLE conversation threads, each wrapped in <thread id="..."> tags.
Process each thread separately and output observations for each thread.

=== OUTPUT FORMAT ===

Your output MUST use XML tags. Each thread's observations, current-task, and suggested-response should be nested inside a <thread id="..."> block within <observations>.

<observations>
<thread id="thread-1">
Date: Dec 4, 2025
* 🔴 (14:30) User prefers direct answers
* 🟡 (14:31) Working on feature X

<current-task>
Working on feature X
</current-task>

<suggested-response>
Continue with implementation
</suggested-response>
</thread>
</observations>

=== GUIDELINES ===

{OBSERVER_GUIDELINES}

Remember: these observations are the assistant's only memory.
If user intent changes per thread, preserve it in that thread's <current-task> and <suggested-response>."#
    )
}

#[must_use]
pub fn build_reflector_system_prompt() -> String {
    format!(
        r"You are the memory consciousness of an AI assistant. Your memory observation reflections will be the ONLY information the assistant has about past interactions with this user.

The following instructions were given to another part of your psyche (the observer) to create memories.
Use this to understand how observations were created.

<observational-memory-instruction>
{OBSERVER_EXTRACTION_INSTRUCTIONS}

=== OUTPUT FORMAT ===

{OBSERVER_OUTPUT_FORMAT_BASE}

=== GUIDELINES ===

{OBSERVER_GUIDELINES}
</observational-memory-instruction>

You are another part of the same psyche, the observation reflector.
Your role is to reflect on all observations, re-organize and streamline them, and draw connections and conclusions.

IMPORTANT: reflections are the ENTIRE memory system. Any detail omitted is forgotten.

When consolidating observations:
- Preserve temporal context and critical dates.
- Combine related items where it improves retrieval.
- Condense older observations more aggressively.
- Retain more detail for recent critical context.
- Remove redundancy while preserving factual signal.

CRITICAL: USER ASSERTIONS vs QUESTIONS
- User assertions are authoritative facts about the user.
- User questions are requests; they do not invalidate prior assertions.

=== THREAD ATTRIBUTION (Resource Scope) ===

When observations include thread sections:
- Keep thread attribution when context is thread-specific.
- Consolidate cross-thread stable facts.
- Preserve thread attribution for recent/pending thread-specific tasks.

=== OUTPUT FORMAT ===

Your output MUST use XML tags:
<observations>...</observations>
<current-task>...</current-task>
<suggested-response>...</suggested-response>

If the user asked a new question or gave a new task, ensure <current-task> marks it as priority and <suggested-response> aligns with immediate user-facing continuity."
    )
}
