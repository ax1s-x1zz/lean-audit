# AGENTS.md

`lean-audit` 저장소에서 작업하는 모든 에이전트/사람이 따르는 규칙.

## 검증 명령 (필수)

변경 후 반드시 통과해야 한다.

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

릴리스 전 추가:

```sh
cargo build --release
./target/release/lean-audit inspect ../team42-repos/coth-sums-barnes-lean
```

## 커밋 정책

- 자율 커밋 허용. 메시지는 Conventional Commits.
  - `feat(inspect): ...`, `fix(graph): ...`, `docs: ...`, `test: ...`, `chore: ...`
- 한 커밋 = 한 논리적 변경.
- 커밋 본문에 **무엇을 검증했는지**(실행한 명령과 결과)를 한 줄로 남긴다.

## 코드 관례

- edition 2024, `rust-version = 1.88` (let-chains을 쓰므로 1.88 이상 필요).
- 의존성 최소화. 새 의존성 추가는 PR 본문에 이유를 적는다.
- `unsafe` 금지.
- 사용자에게 보이는 모든 문자열은 **정직성 규칙**을 따른다 (아래).
- 오류는 `anyhow::Result`, 사용자 입력 오류는 `anyhow::bail!`.
- 파일 파싱 실패 시 **추측하지 않고** 상태를 `unsupported` / `error`로 기록한다.

## 정직성 규칙 (이 프로젝트의 존재 이유)

1. `not_run`을 `ok`로 표기하지 않는다. 실행하지 않은 검사는 `not_run`이다.
2. `sorry`를 **문자열 검색으로 판정하지 않는다**. 커널/환경 기반 판정이 불가능하면 그 사실을 기록한다.
3. 이 도구는 **수학적 참·거짓을 판정하지 않는다**. 모든 리포트에 disclaimer를 포함한다.
4. 기존 `Audit.lean` / `AxiomCheck.lean` / `scripts/audit_axioms.py`를 **대체한다고 주장하지 않는다**.
5. 검사 결과를 **저장소에 대한 비난으로 표현하지 않는다**. `mismatch`는 "주장과 재계산이 다름"일 뿐이다.

## 테스트 관례

- 순수 파일 파싱 로직은 `#[cfg(test)] mod tests`로 단위 테스트.
- `src/lean_src.rs`의 `lean_code()`(주석·문자열 제거)는 **음성 테스트 필수**:
  주석/문자열 안의 토큰이 탐지되지 않아야 한다.
- 실제 저장소에 의존하는 통합 테스트는 `tests/`에 두고, 저장소가 없으면 skip한다.
