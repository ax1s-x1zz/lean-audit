# M0 — 요구사항: 기존 감사 구현 3종 분석

> 작성일 2026-11-17 · 근거: 실제 소스 직접 열람
> 목적: `lean-audit`이 **무엇을 대체하지 않고**, **무엇을 새로 제공하는지**를 코드 수준에서 확정한다.

---

## 1. 분석 대상 (실제 파일)

| 저장소 | 파일 | 줄 수 | 방식 |
|---|---|---|---|
| `sierksma33` | `Audit.lean` | 54 | Lean elaborator command `#sierksma_audit` |
| `rubik-stm-entropy-lean` | `AxiomCheck.lean` | 64 | Lean elaborator command `#check_axioms` |
| `site-percolation-square-lattice-lean` | `scripts/audit_axioms.py` | 117 | Python → 임시 `.lean` 생성 → `lake env lean` → stdout 파싱 |

---

## 2. `sierksma33/Audit.lean` 동작

```lean
import SierksmaLean.Main
import Solution
import Lean
open Lean Elab Command

elab "#sierksma_audit" : command => do
  let env ← getEnv
  let std : List Name := [``propext, ``Classical.choice, ``Quot.sound]
  let skeleton : Bool := false
  let expected : List Name := []                       -- 직접 sorryAx 사용 상수 (기대: 없음)
  let heads : List Name := [``SierksmaLean.main_theorem, …]
  for h in heads do
    let axs := (← liftCoreM <| Lean.collectAxioms h).toList
    logInfo m!"AUDIT axioms {h}: {axs}"
    for a in axs do
      unless std.contains a || (skeleton && a == ``sorryAx) do
        throwError m!"AUDIT FAIL {h}: forbidden axiom {a}"
    unless skeleton == axs.contains ``sorryAx do
      throwError m!"AUDIT FAIL {h}: sorryAx presence does not match the placeholder list"
  let mods := env.header.moduleNames
  let direct := env.constants.map₁.fold (init := (#[] : Array Name)) fun acc n ci =>
    match env.getModuleIdxFor? n with
    | some idx =>
      if mods[idx.toNat]!.getRoot == `SierksmaLean && ci.getUsedConstantsAsSet.contains ``sorryAx then
        acc.push n
      else acc
    | none => acc
  let found := (direct.qsort Name.lt).toList
  let want := (expected.toArray.qsort Name.lt).toList
  logInfo m!"AUDIT placeholders ({found.length}): {found}"
  unless found == want do
    throwError m!"AUDIT FAIL placeholder mismatch: found {found}, expected {want}"

#sierksma_audit
```

**핵심 API**
- `Lean.collectAxioms h` — 정리의 공리 의존성 (커널 기준, 문자열 아님)
- `env.constants.map₁` — 환경 전체 상수 순회
- `env.getModuleIdxFor? n` — 상수 → 모듈 인덱스
- `env.header.moduleNames` — 모듈 이름 목록
- `ci.getUsedConstantsAsSet.contains ``sorryAx` — **직접** `sorry` 사용 여부

**실패 조건**: ① 허용 외 공리가 있으면 `throwError`, ② `sorryAx` 직접 사용 목록이 기대와 다르면 `throwError`.
**특징**: `defaultTargets`에 포함되어 **빌드가 실패**한다. 즉 감사가 빌드의 일부다.

---

## 3. `rubik/AxiomCheck.lean` 동작

같은 구조에 차이가 있다.

| 항목 | `Audit.lean` | `AxiomCheck.lean` |
|---|---|---|
| 명령 이름 | `#sierksma_audit` | `#check_axioms` |
| 헤드 표현 | `List Name` | `List (Name × Bool)` (이름, allowSorry) |
| 헤드 개수 | 3 | **30** |
| `expected` (직접 sorry) | `[]` | `[]` |
| 루트 필터 | `` `SierksmaLean `` | `` `RubikSTM `` |
| 출력 접두사 | `AUDIT axioms` / `AUDIT placeholders` | 동일 |

**→ 같은 일을 하는 코드가 저장소마다 이름·자료구조·헤드 목록이 다르게 재작성되어 있다.**

---

## 4. `scripts/audit_axioms.py` 동작 (참조 명세)

117줄. 단계별로:

1. **인자 검증** — `--module`, `--theorem`, `--allow-native`가 정규식 `[A-Za-z_][A-Za-z_0-9.']*`에 맞는지 확인. 아니면 `parser.error`.
2. **소스 스캔** — 기본 대상 `SitePerc/**/*.lean`. `lean_code()`로 주석·문자열을 제거한 뒤
   `\b(?:sorry|admit|axiom|unsafe|implemented_by|extern)\b` 를 찾아 `{path, line, token}` 수집.
3. **감사 프로그램 생성**
   ```lean
   import <module>
   …
   #print axioms <theorem>
   …
   ```
4. **실행** — `lake env lean <tmp>/Audit.lean`, `cwd = 저장소 루트`, `capture_output=True`.
5. **파싱** — stdout+stderr에서
   - `'X' depends on axioms: [a, b]`
   - `'X' does not depend on any axioms`
6. **판정**
   - `missing` = 요청했으나 출력에 없는 정리
   - `unexpected` = `STANDARD ∪ allow_native` 밖의 공리
   - `pass` = `returncode == 0` **그리고** `flags` 비어 있고 **그리고** `missing` 비어 있고 **그리고** `unexpected` 비어 있음
7. **출력** — JSON (`compiler_output` 원문 포함), `pass` 아니면 exit 1.

### 4.1 `lean_code()` — 왜 필요한가

```
"Remove nested comments and strings without joining adjacent tokens."
```

- **중첩 블록 주석** `/- /- … -/ -/` 을 depth 카운터로 처리
- `--` 줄 주석
- `"` 문자열 + `\` 이스케이프
- 제거된 내용을 **공백/개행으로 대체**해 토큰이 붙지 않게 함 (줄 번호 보존)
- 종료되지 않은 블록 주석은 `ValueError`

**이 함수가 없으면** 주석·문자열 안의 `sorry`/`axiom`이 **오탐**이 된다. 이것이 "문자열 검색 금지" 원칙의 실제 구현이다.

### 4.2 알려진 한계 (그대로 보존할 것)

| 한계 | 영향 | 정책 |
|---|---|---|
| 작은따옴표 문자 리터럴(`'a'`) 미처리 | Lean 문자 리터럴은 1글자이므로 `sorry` 등이 들어갈 수 없음 → **영향 없음** | 그대로 |
| term quotation `` `(sorry) `` 미처리 | 매크로 인용 안의 토큰이 **탐지됨** | **보수적 방향이므로 유지**. 정책은 `docs/method.md`에 명시 |
| 정규식 `\b` | 유니코드 식별자 경계는 부정확 | Rust 구현에서는 **식별자 토큰화**로 대체 (더 정확) |
| `#print axioms` 출력 형식 의존 | Lean 메시지 포맷이 바뀌면 파싱 실패 | `collectAxioms` 직접 출력 모드를 **기본**으로, `#print axioms` 파싱을 **호환 모드**로 |

---

## 5. 3종 구현의 공통 계약 (도구가 지켜야 할 것)

| 항목 | 값 |
|---|---|
| 표준 공리 | `propext`, `Classical.choice`, `Quot.sound` |
| 직접 `sorry` 사용 | 저장소 라이브러리 루트 하위에서 **0개** (기대 목록 `[]`) |
| 금지 토큰 | `sorry`, `admit`, `axiom`, `unsafe`, `implemented_by`, `extern` |
| 판정 방식 | 커널/환경 기반 (`collectAxioms`, `env.constants`) — **문자열 검색 아님** |
| 실패 시 | 빌드 실패 또는 exit 1 |

---

## 6. `lean-audit`이 제공할 것 / 제공하지 않을 것

### 제공
1. **외부 실행** — 저장소를 수정하지 않고, `defaultTargets`에 들어가지 않고, 감사한다.
2. **다중 저장소** — 9개 저장소를 한 번에. 툴체인(v4.33.1 vs v4.35.0)·Mathlib 핀 드리프트 가시화.
3. **증분·저비용** — 클린 빌드(33 GB / 39분) 없이 `lake env lean` 단일 파일 실행.
4. **구조화 출력** — JSON 스키마 v1 + Markdown. 실행 간 `drift` diff.
5. **주장 검증** — `SOURCE_MANIFEST.json` / `MANIFEST.sha256` 해시 재계산, `receipts/` 수치와 재실행 결과 비교.
6. **공통 계약의 단일 구현** — 새 저장소가 생겨도 감사 코드를 다시 짤 필요 없음.
7. **탐지 강화(선택)** — 환경 기반으로 `axiom` 선언 수·`unsafe` 상수 수를 셀 수 있음 (정규식보다 정확).

### 제공하지 않음
1. ❌ 수학적 참·거짓 판정
2. ❌ 기존 3종의 대체·삭제
3. ❌ Comparator 대체 (5시간 재검증은 Comparator의 일)
4. ❌ 클린 빌드 강제
5. ❌ `sorry` 문자열 검색

---

## 7. `inspect` (M1) 요구사항

Lean 실행 **없이** 파일만으로 결정 가능한 것들.

| 필드 | 출처 | 비고 |
|---|---|---|
| `lean_toolchain` | `lean-toolchain` | `leanprover/lean4:v4.33.1` 형식 파싱 |
| `lakefile` | `lakefile.toml` | `name`, `version`, `defaultTargets`, `require[]`, `lean_lib[]` |
| `lakefile_lean` | `lakefile.lean` 존재 여부 | 존재하면 `unsupported` 표기 (파싱 안 함) |
| `mathlib_rev` / `mathlib_tag` | `lake-manifest.json` + `lakefile.toml` require | |
| `modules` | `**/*.lean` | `lake-packages/`, `.lake/` 제외 |
| `import_edges` | 각 `.lean`의 `import` 문 | **`lean_code()` 적용 필수** |
| `external_imports` | 로컬 모듈 집합에 없는 import | Mathlib 등 |
| `roots` | 피참조가 없는 모듈 | 보통 `Challenge`/`Solution`/`Audit`/`Check`/`Main` |
| `max_import_depth` | DAG 최장 경로 | 순환은 오류로 보고 |
| `audit_impl` | `Audit.lean` / `AxiomCheck.lean` / `scripts/audit_axioms.py` | 존재 여부 + **헤드 정리 이름 추출** |
| `manifests` | `SOURCE_MANIFEST.json`, `MANIFEST.sha256`, `data/AXIOM_AUDIT.json` | 존재 + 항목 수 (해시 검증은 M2) |
| `receipts` | `receipts/` | 파일 목록 + `build_summary.txt` 파싱 |

### 7.1 헤드 정리 이름 추출

`Audit.lean` / `AxiomCheck.lean`에서 감사 대상 이름을 뽑는다.
- 패턴: `` ``Name `` (double-backtick Name literal), `[``A, ``B]` 목록, `List Name := [...]`
- 추출 실패 시 `heads: null` + `heads_source: "unparsed"` — **추측하지 않는다**

### 7.2 `import` 문 파싱

`lean_code()` 적용 후 각 줄에서:
- `import X.Y.Z` / `public import X.Y.Z` / `import all X` / `import A B`
- 토큰 `all`은 무시
- 하나의 import 줄에 여러 모듈이 있으면 모두 수집

---

## 8. 정직성 상태값

| 값 | 의미 |
|---|---|
| `ok` | 재계산 결과가 주장과 일치 |
| `mismatch` | 주장과 다름 (저장소에 대한 비난이 아님) |
| `not_run` | 실행하지 않음 — **절대 `ok`로 표기하지 않음** |
| `parsed_only` | 읽기만 함, 재계산 안 함 |
| `unsupported` | 구조가 예상과 다름 (추측 금지) |
| `error` | 도구 자체 오류 |
