## 결론

비교 제품이나 외부 댓글은 빼고, **AxiomMe + `episodic` 현재 구현만 기준**으로 보면 OM(Observational Memory) 쪽에서 바로 개선 가치가 큰 지점이 분명히 있습니다.

가장 우선순위가 높은 것은 6가지입니다.

1. **검색이 buffered observation을 못 본다**
2. **interval-triggered observer가 `current_task`/`suggested_response`를 갱신하지 않아 task continuity가 늦게 반영된다**
3. **`episodic`가 제공하는 `request_json` 기반 구조화 프롬프트 기능을 AxiomMe가 거의 안 쓰고 있다**
4. **resource scope에서 preferred thread 선택이 잘못될 가능성이 높다**
5. **deterministic fallback이 너무 약해서 LLM off/실패 시 OM 품질이 급격히 떨어진다**
6. **reflection merge가 line-count 기반이라 장기적으로 중복/과삭제가 발생할 수 있다**

추가로, 현재 AxiomMe는 `episodic = "0.1.0"`에 의존하지만 `episodic` 저장소의 현재 버전은 `0.1.1`이라서, 두 레포를 함께 발전시키는 동안 **contract drift**가 생길 가능성도 있습니다. ([GitHub][1])

---

## 1) 검색이 buffered observation을 못 본다

### 왜 문제인가

AxiomMe의 search 쪽 OM 힌트는 `build_om_hint_state_from_record()`에서 **`record.active_observations`와 `suggested_response`만** 사용해 만듭니다. 반면 OM 상태 저장소에는 `om_observation_chunks`라는 별도 buffered chunk 테이블이 있고, write-path / async observe path는 여기에 observation chunk를 append한 뒤 나중에 activate합니다. 즉 **새로 관찰된 내용이 buffer에만 있고 아직 activate되지 않은 동안엔 검색 힌트에서 사라집니다.** ([GitHub][2])

이건 AxiomMe의 search 품질에 직접 연결됩니다. 현재 hint builder는 `active_observations`만 flatten해서 쓰므로, **최신 작업 맥락이 search query plan에 늦게 반영**될 수 있습니다. ([GitHub][2])

### 어떻게 고칠지

검색용 OM state를 만들 때 buffered chunk를 같이 합치면 됩니다.

#### 최소 수정안

* `fetch_om_state_by_scope_key*()` 내부에서

  * `list_om_observation_chunks(record.id)` 호출
  * 최신 1~2개 chunk만 가져와
  * `active_observations + buffered_tail` 조합으로 search hint 생성

```rust
fn build_search_visible_observations(
    active: &str,
    buffered: &[OmObservationChunk],
    max_chars: usize,
) -> String {
    let mut parts = Vec::new();

    if !active.trim().is_empty() {
        parts.push(active.trim().to_string());
    }

    let buffered_tail = buffered.iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|c| c.observations.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if !buffered_tail.is_empty() {
        parts.push(format!("--- BUFFERED ---\n{buffered_tail}"));
    }

    parts.join("\n\n").chars().take(max_chars).collect()
}
```

#### 더 나은 수정안

`OmHintReadStateV1`에 `buffered_hint`를 추가하고, search merge 단계에서 `active`와 `buffered`를 별도 레이어로 다루세요.

### 검증

* `buffer_only_visibility_in_search_hint`
* `search_hint_includes_latest_buffered_observation_before_activation`

---

## 2) interval-triggered observer가 task continuity를 늦게 갱신한다

### 왜 문제인가

`episodic`의 `should_skip_observer_continuation_hints()`는 **interval-triggered이지만 threshold에 도달하지 않은 경우** continuation hints를 건너뛰게 설계되어 있습니다. AxiomMe의 async observe path는 실제로 `skip_continuation_hints: true`를 사용합니다. 그래서 이 경로에선 `current_task`와 `suggested_response`가 갱신되지 않습니다. 결과적으로 **관찰은 쌓이는데 “지금 뭘 하는지” 힌트는 늦게 따라옵니다.** ([GitHub][3])

search 쪽은 OM hint를 만들 때 `suggested_response`를 함께 붙일 수 있게 되어 있는데, async observe가 그 필드를 안 갱신하면 search context가 stale해질 수 있습니다. ([GitHub][2])

### 어떻게 고칠지

#### 권장안

`current_task`와 `suggested_response`를 분리하세요.

* async / interval path:

  * `current_task`는 갱신 허용
  * `suggested_response`는 기존처럼 보수적으로 유지
* sync / threshold/block_after path:

  * 둘 다 갱신

예시:

```rust
#[derive(Debug, Clone, Copy)]
struct ContinuationHintPolicy {
    allow_current_task: bool,
    allow_suggested_response: bool,
}

fn continuation_policy(decision: ObserverWriteDecision, async_path: bool) -> ContinuationHintPolicy {
    if async_path || (decision.interval_triggered && !decision.threshold_reached) {
        return ContinuationHintPolicy {
            allow_current_task: true,
            allow_suggested_response: false,
        };
    }

    ContinuationHintPolicy {
        allow_current_task: true,
        allow_suggested_response: true,
    }
}
```

### 검증

* `interval_observer_updates_current_task_without_suggested_response`
* `async_observe_does_not_leave_task_hint_stale`

---

## 3) `episodic`의 구조화 프롬프트 기능을 AxiomMe가 충분히 안 쓴다

### 왜 문제인가

`episodic`는 pure-layer에서 `OmObserverRequest`, `OmReflectorRequest`, `OmObserverPromptInput`, `OmReflectorPromptInput`을 정의하고, prompt builder는 `request_json`을 받을 수 있게 설계돼 있습니다. 그런데 AxiomMe의 observer LLM 호출 경로를 보면 실제 prompt를 만들 때 **`request_json: None`**을 넘깁니다. 즉 이미 런타임에 있는 구조화 DTO를 프롬프트에 거의 전달하지 않습니다. ([GitHub][4])

이건 정확도 문제입니다. 지금도 parser가 JSON/XML fallback을 꽤 넓게 받아주지만, **모델이 “무엇을 반환해야 하는지”를 구조적으로 더 명시할 수 있는데 그 수단을 버리고 있는 상태**입니다. ([GitHub][5])

### 어떻게 고칠지

observer prompt에 최소한 아래 JSON을 넣으세요.

```json
{
  "scope": "resource",
  "scope_key": "resource:src/lib.rs",
  "known_message_ids": ["m1", "m2", "m3"],
  "skip_continuation_hints": false,
  "output_contract": {
    "format": "xml",
    "sections": ["observations", "current-task", "suggested-response"]
  },
  "limits": {
    "max_output_tokens": 1200,
    "observation_max_chars": 4000
  }
}
```

AxiomMe patch 포인트는 여기입니다.

```rust
let request_json = serde_json::to_string_pretty(&serde_json::json!({
    "scope": request.scope.as_str(),
    "scope_key": request.scope_key,
    "known_message_ids": known_ids,
    "skip_continuation_hints": skip_continuation_hints,
    "model": request.model,
}))?;

let user_prompt = build_observer_user_prompt(OmObserverPromptInput {
    request_json: Some(&request_json),
    existing_observations: Some(&request.active_observations),
    message_history: &message_history,
    other_conversation_context: request.other_conversations.as_deref(),
    skip_continuation_hints,
});
```

### 검증

* `observer_prompt_includes_request_json`
* `strict_llm_mode_schema_error_rate_drops`
* `parse_llm_observer_response_acceptance_rate_improves`

---

## 4) resource scope에서 preferred thread 선택이 어긋날 가능성이 높다

### 왜 문제인가

resource scope의 multi-thread observer는 thread grouping 시 `source_thread_id`를 우선 쓰고, 없으면 `source_session_id`, 그것도 없으면 fallback을 씁니다. 그런데 batch 결과를 합칠 때 preferred thread 선택은 `current_session_id` 기준으로 합니다. 즉 **실제 thread key가 `source_thread_id`인데 preferred selection은 session id로 찾는 구조**입니다. 이 경우 `current_task` / `suggested_response`가 현재 스레드가 아니라 다른 스레드에서 뽑힐 수 있습니다. ([GitHub][6])

이 문제는 thread state upsert 경로에서도 비슷하게 보입니다. resource scope에서 primary thread를 정할 때도 session id 기반 fallback이 섞여 있습니다. 구조상 **현재 thread와 session id가 다를 때 mismatch가 생길 여지**가 있습니다. ([GitHub][6])

### 어떻게 고칠지

`current_session_id`와 별도로 `preferred_thread_id`를 명시적으로 넘기세요.

```rust
struct MultiThreadObserverRunContext<'a> {
    request: &'a OmObserverRequest,
    bounded_selected: &'a [OmObserverMessageCandidate],
    thread_messages: &'a [OmObserverThreadMessages],
    scope: OmScope,
    scope_key: &'a str,
    current_session_id: &'a str,
    preferred_thread_id: Option<&'a str>,
    max_tokens_per_batch: u32,
    skip_continuation_hints: bool,
}
```

그리고 여기서:

```rust
let preferred = context.preferred_thread_id.unwrap_or(context.current_session_id);

let current_task = preferred_thread_text(
    &combined_thread_states,
    preferred,
    ObserverThreadField::CurrentTask,
);
```

### 검증

* `resource_scope_prefers_explicit_thread_id_for_current_task`
* `resource_scope_does_not_fall_back_to_unrelated_session_thread`

---

## 5) deterministic fallback이 약하다

### 왜 문제인가

AxiomMe는 observer model이 꺼져 있거나 실패하면 deterministic path로 떨어집니다. 그런데 그 path는 pending messages를 normalize해서 observation text만 만들고, **`current_task`와 `suggested_response`는 항상 `None`**입니다. 즉 LLM이 꺼지거나 strict mode에서 실패했을 때, OM은 “요약 문자열”만 남고 task continuity 정보가 사라집니다. ([GitHub][7])

search layer는 `suggested_response`가 있으면 hint에 `next:` 형태로 붙이도록 되어 있습니다. 따라서 deterministic fallback의 빈 continuation fields는 search usability도 같이 떨어뜨립니다. ([GitHub][2])

### 어떻게 고칠지

`episodic` pure transform에 **경량 rule-based continuation extractor**를 넣는 것이 맞습니다.

#### 추천 규칙

* 마지막 user imperative 문장 → `current_task`
* “fix/add/update/implement/investigate” 동사 + 대상 noun phrase → `current_task`
* assistant/tool 실패 메시지 + pending user ask → `suggested_response`
* 파일명/함수명/에러 코드/설정 키는 별도 보존

예시:

```rust
pub fn infer_continuation_hints_deterministic(
    pending: &[OmPendingMessage],
) -> (Option<String>, Option<String>) {
    let last_user = pending.iter().rev().find(|m| m.role == "user");
    let last_tool = pending.iter().rev().find(|m| m.role == "tool");

    let current_task = last_user
        .and_then(|m| extract_task_phrase(&m.text));

    let suggested_response = match (last_user, last_tool) {
        (Some(user), Some(tool)) if contains_error_signal(&tool.text) => {
            Some(format!("Address error and continue: {}", summarize_request(&user.text)))
        }
        (Some(user), _) => extract_next_step(&user.text),
        _ => None,
    };

    (current_task, suggested_response)
}
```

### 검증

* `deterministic_fallback_emits_current_task`
* `deterministic_fallback_preserves_error_context_identifiers`

---

## 6) reflection merge가 line-count 기반이라 장기적으로 취약하다

### 왜 문제인가

`episodic`의 reflection 쪽은 `build_reflection_draft()`에서 line count를 세고, `plan_buffered_reflection_slice()`도 평균 token/line 기반으로 slice를 잡습니다. 그리고 AxiomMe는 reflection apply 시 `reflected_observation_line_count`를 써서 앞쪽 N줄을 reflection으로 대체합니다. 이 구조는 **모델이 줄을 합치거나 쪼개는 순간 중복 또는 과삭제**가 생기기 쉽습니다. ([GitHub][8])

즉 지금 구조는 “텍스트 압축”으로는 간단하지만, 장기 실행에서 observation 품질을 보존하는 데이터 모델로는 약합니다.

### 어떻게 고칠지

#### 단기안

각 observation line 앞에 stable id를 붙이세요.

```text
[obs:01HF7...] User prefers direct answers
[obs:01HF8...] Working on queue replay bug
```

reflection은 다음 형식으로 반환하게 바꿉니다.

```xml
<observations>
  <covers>obs:01HF7...,obs:01HF8...</covers>
  - User prefers direct answers and is debugging queue replay.
</observations>
```

#### 장기안

flat text 대신 structured entry 저장:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OmObservationEntry {
    id: String,
    ts: String,
    priority: ObservationPriority,
    text: String,
    source_message_ids: Vec<String>,
    thread_id: Option<String>,
}
```

그 다음:

* 표시용 텍스트는 render
* reflection merge는 `covers: [entry_id...]` 기준으로 적용

### 검증

* `reflection_merge_does_not_duplicate_when_model_rewraps_lines`
* `reflection_merge_preserves_uncovered_entries`

---

## 7) hint compressor가 priority semantics를 버린다

### 왜 문제인가

`episodic`의 system prompt는 High/Medium/Low priority, current-task, suggested-response를 명시적으로 다루라고 지시합니다. 그런데 실제 AxiomMe search hint builder는 `build_bounded_observation_hint()`로 **마지막 몇 줄만 whitespace-normalize해서 평탄화**합니다. 여기에 search config 기본값은 total hint 2개, hint 본문 480 chars, 4 lines입니다. 결국 **priority가 높지만 tail에 없던 정보는 쉽게 탈락**합니다. ([GitHub][9])

### 어떻게 고칠지

tail-only 대신 **priority-aware selection**으로 바꾸세요.

#### 추천 알고리즘

1. `<current-task>` / `suggested-response` 최우선
2. `High:` 또는 🔴 표시 line 우선
3. 그다음 최근 tail
4. 마지막으로 char budget trim

```rust
fn select_search_hint_lines(lines: &[String], max_lines: usize) -> Vec<String> {
    let mut high = Vec::new();
    let mut normal = Vec::new();

    for line in lines {
        let t = line.trim();
        if t.contains("High:") || t.contains("🔴") {
            high.push(t.to_string());
        } else {
            normal.push(t.to_string());
        }
    }

    high.into_iter()
        .chain(normal.into_iter().rev())
        .take(max_lines)
        .collect()
}
```

### 검증

* `high_priority_observation_survives_hint_compaction`
* `current_task_line_is_never_evicted_by_recent_low_value_tail`

---

## 8) `episodic` 버전 drift를 먼저 정리하는 게 좋다

### 왜 문제인가

AxiomMe는 `episodic = "0.1.0"`에 의존하고 있고, `om/mod.rs`에서 pure contract를 대량 re-export합니다. 그런데 `episodic` 저장소의 현재 `Cargo.toml`은 `0.1.1`입니다. 두 레포를 동시에 발전시키는 동안엔 **AxiomMe 런타임과 episodic pure-layer가 서로 다른 계약을 보고 있을 수 있습니다.** ([GitHub][1])

### 어떻게 고칠지

개발 중에는 아래 둘 중 하나를 추천합니다.

#### 1) 로컬 co-dev

```toml
[patch.crates-io]
episodic = { path = "../episodic" }
```

#### 2) CI 재현성 유지

```toml
[dependencies]
episodic = { git = "https://github.com/AxiomOrient/episodic", rev = "<commit>" }
```

그리고 AxiomMe 쪽에 **contract snapshot tests**를 추가하세요.

* prompt input/output golden
* parse golden
* observer decision golden
* reflection merge golden

---

## 적용 우선순위

### 바로 할 것

1. **search에 buffered observation 노출**
2. **async observer에서도 `current_task`는 갱신**
3. **resource scope preferred thread id 수정**
4. **observer prompt에 `request_json` 주입**
5. **`episodic` version drift 정리**

### 그 다음

6. **deterministic continuation extractor 추가**
7. **priority-aware hint compaction**
8. **reflection을 line-count → stable entry id 기반으로 전환**

---

## 추천 테스트 세트

```bash
cargo test om_search_includes_buffered_chunks
cargo test om_async_updates_current_task
cargo test om_resource_scope_prefers_explicit_thread
cargo test om_observer_prompt_contains_request_json
cargo test om_deterministic_fallback_emits_continuation_hints
cargo test om_reflection_merge_by_entry_id
```

golden fixture는 최소 이 5종이 필요합니다.

* session scope, 짧은 대화
* resource scope, 여러 thread/session 혼합
* async observe만 여러 번 발생하는 케이스
* LLM 실패 후 deterministic fallback 케이스
* reflection이 line merge/split를 일으키는 케이스

---

[1]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/Cargo.toml "raw.githubusercontent.com"
[2]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/client/search/mod.rs "raw.githubusercontent.com"
[3]: https://raw.githubusercontent.com/AxiomOrient/episodic/main/src/transform/observer/decision.rs "raw.githubusercontent.com"
[4]: https://raw.githubusercontent.com/AxiomOrient/episodic/main/src/inference.rs "raw.githubusercontent.com"
[5]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/session/om/observer/parsing.rs "raw.githubusercontent.com"
[6]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/session/om/observer/threading.rs "raw.githubusercontent.com"
[7]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/session/om/observer/response.rs "raw.githubusercontent.com"
[8]: https://raw.githubusercontent.com/AxiomOrient/episodic/main/src/transform/reflection/draft.rs "raw.githubusercontent.com"
[9]: https://raw.githubusercontent.com/AxiomOrient/episodic/main/src/prompt/system.rs "raw.githubusercontent.com"
[10]: https://raw.githubusercontent.com/AxiomOrient/AxiomMe/main/crates/axiomme-core/src/om/mod.rs "raw.githubusercontent.com"
