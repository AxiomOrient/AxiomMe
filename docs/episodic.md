# episodic Integration Note

## 목적

AxiomMe에서 OM(Observational Memory) 순수 모델/변환을 `episodic`로 표준화하고,
AxiomMe는 런타임 실행 책임(파일시스템, sqlite, outbox, session)을 담당한다.

## 현재 결정 (2026-02-22)

- 기본 OM 엔진: `episodic`
- 통합 경계: `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/om/mod.rs`
- 의존성 선언: `/Users/axient/repository/AxiomMe/crates/axiomme-core/Cargo.toml`
- 로컬 OM 복제 구현 제거: `/Users/axient/repository/AxiomMe/crates/axiomme-core/src/om`에는 경계 파일(`mod.rs`, `failure.rs`, `rollout.rs`)만 유지

즉, `axiomme-core::om`은 런타임이 소비하는 단일 OM 계약 표면이며,
구현의 순수 부분은 `episodic`에서 직접 재수출한다.

## 경계 원칙

- 데이터 모델 우선: `OmRecord`, `OmObservationChunk`, `OmScope` 등 계약 타입을 먼저 고정한다.
- 순수/효과 분리: OM 계산은 `episodic`, 상태 반영/재시도/큐 처리는 `axiomme-core`에서 수행한다.
- 중복 제거: 동일 OM 로직의 in-repo 복제본을 기본 구현으로 두지 않는다.
- 명시적 비용: 토큰 경계, 배치 경계, CAS generation 경계는 런타임에서 명시적으로 관리한다.

## 런타임 책임 (AxiomMe)

- sqlite OM 테이블 읽기/쓰기
- outbox 이벤트 enqueue/replay/dead-letter
- session/thread/resource scope 바인딩
- 실패 분류와 재시도 정책

## 순수 책임 (episodic)

- OM 타입/불변식
- observer/reflector 입력 계획
- parsing/normalization/merge 규칙
- trigger/decision 계산

## 브리지 정책

추가 브리지는 선택 사항이다.
기본 동작은 이미 직접 통합되어 있으며, 별도 JSON 브리지는 외부 도구/분석 파이프라인이 필요할 때만 둔다.

## 검증 명령

```bash
cargo test -p axiomme-core --manifest-path /Users/axient/repository/AxiomMe/Cargo.toml
cargo test -p axiomme-cli --manifest-path /Users/axient/repository/AxiomMe/Cargo.toml
cargo check --workspace --manifest-path /Users/axient/repository/AxiomMe/Cargo.toml
```

## 변경 시 체크리스트

- `axiomme-core::om` 재수출 표면과 `episodic` 공개 API가 일치하는지 확인
- OM 스키마와 런타임 적용 코드(CAS 포함) 회귀 테스트 통과 확인
- 문서(`README.md`, `docs/FEATURE_SPEC.md`, `docs/README.md`)의 경계 설명 동기화
