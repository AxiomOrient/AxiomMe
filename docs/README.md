# Documentation Index

이 저장소의 문서는 canonical 문서와 archive 문서를 분리해 관리합니다.

## Canonical
- [README.md](../README.md)
- [docs/ARCHITECTURE.md](ARCHITECTURE.md)
- [docs/FEATURE_SPEC.md](FEATURE_SPEC.md)
- [docs/API_CONTRACT.md](API_CONTRACT.md)
- [docs/RELEASE_NOTES.md](RELEASE_NOTES.md)
- [docs/FEATURE_COMPLETENESS_UAT_GATE.md](FEATURE_COMPLETENESS_UAT_GATE.md)
- [docs/RELEASE_SIGNOFF_REQUEST.md](RELEASE_SIGNOFF_REQUEST.md)
- [docs/RELEASE_SIGNOFF_STATUS.md](RELEASE_SIGNOFF_STATUS.md)
- [docs/IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md)
- [docs/TASKS.md](TASKS.md)

## Supplemental
- [docs/ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md](ONTOLOGY_SCHEMA_EVOLUTION_POLICY.md)

## Archive
- 과거 날짜/버전 기반 산출물은 [docs/archive](archive) 하위에 보관합니다.
- CI/운영에서 재생성되는 리포트는 가능한 `logs/` 경로를 우선 사용합니다.
- 저장소 내에서 더 이상 참조되지 않는 archive 문서는 주기적으로 제거합니다.

## Rules
- 문서 내 절대 사용자 경로(`/Users/<name>/...`)는 canonical 문서에서 사용하지 않습니다.
- 날짜/버전 식별자는 릴리즈 계약에 필요한 경우에만 유지합니다.
- API/동작 정의의 단일 진실 공급원은 `FEATURE_SPEC.md`와 `API_CONTRACT.md`입니다.
