# Technical Benchmarking Strategy

This document outlines the strategy for running the final technical benchmarking on the command-finder application. The primary goal of this benchmarking is to measure the accuracy of the command generation tool by running a set of queries and evaluating the correctness of the generated tool and option flags against a ground-truth dataset.

---

## 1. Objectives

1. **Evaluate Tool Selection Accuracy:** Verify if the correct command/tool (e.g., `mv`, `cp`, `ops-sync`) is resolved.
2. **Evaluate Option Flag Precision & Recall:** Measure the presence of expected option flags in the generated commands, including partial matches.
3. **Track Retrieval Robustness:** Quantify the failure rates (errors and unresolved queries) of the orchestrator.

---

## 2. Input Dataset

The benchmark will be executed against [test_set_with_options.csv](file:///home/sandbox-noadmin/RustroverProjects/command-finder/test_set_with_options.csv). Each test case consists of:
* **`ID`**: A unique identifier for the test case.
* **`Original Text`**: The raw natural language query provided by the user.
* **`Category`**: The ground-truth template command (e.g., `mv -iv /source/path /destination/path`). The first token is the **Expected Tool**.
* **`options`**: A JSON-formatted string array of required option flags (e.g., `["-i", "-v"]`). These are the **Expected Options**.

---

## 3. Tool Execution and Output Parsing

### 3.1 Invocation
The benchmark runner will compile the binary in release mode and invoke it for each test query:
```bash
DATABASE_PATH=local_assistant.db target/release/command-finder query "<Original Text>"
```

### 3.2 Output Classification
The resolver [src/core/query_orchestrator.rs](file:///home/sandbox-noadmin/RustroverProjects/command-finder/src/core/query_orchestrator.rs) returns output in one of the following formats:
1. **Success response:** Colored green with ANSI escapes:
   `\u{001b}[1;32m<generated_command>\u{001b}[0m\n\nNote:...`
2. **Disambiguation response:** Colored red with ANSI escapes:
   `\u{001b}[1;31mThe query could not be resolved. Please revise your query.\n...`
3. **Unclear response:** Standard text output:
   `Query unclear`
4. **Execution error:** Command exits with a non-zero status code or prints to stderr.

### 3.3 Output Parsing Pipeline
For each query, the benchmark runner parses the response as follows:
1. Strip all ANSI escape sequences from the tool's stdout and stderr.
2. Check if the run resulted in an **Unsuccessful Retrieval (Error)**:
   * Non-zero process exit code.
   * Stderr contains an error message.
   * Stripped stdout contains `"The query could not be resolved"` or equals `"Query unclear"`.
3. If not an error, extract the **Generated Command**:
   * Clean/strip the green-highlighted command string (i.e. the text between `\u{001b}[1;32m` and `\u{001b}[0m`).
   * Tokenize this command string to find the **Generated Tool** (the first token, e.g. `mv` or `cp`).
   * Extract option flags from the remaining tokens.

---

## 4. Option Flag Matching Logic

Because options can be formatted in multiple ways (e.g., grouped short flags, flag-value assignments), the runner must parse the generated command tokens into a standardized set of **Predicted Options** ($P$).

### 4.1 Token Extraction Rules
For each token in the generated command after the tool name:
* **Long Flags with Arguments:** If a token starts with `--` and contains `=`, split it by `=` and extract the left-hand side (e.g., `--backup=numbered` $\rightarrow$ `--backup`, `--update=older` $\rightarrow$ `--update`).
* **Long Flags without Arguments:** If a token starts with `--` (and has no `=`), extract the token as-is (e.g., `--genesis`).
* **Short Flag Groups:** If a token starts with a single `-` followed by multiple characters (e.g., `-iv` or `-uri`), expand it into separate single-character flags (e.g., `-iv` $\rightarrow$ `-i`, `-v`).
* **Special Delimited Flags:** Check for custom tool patterns, such as `-m:secure` $\rightarrow$ `-m`.
* **Standard Short Flags:** If a token starts with `-` followed by a single character (e.g., `-f`, `-s`), extract it as-is.

### 4.2 Matching Definition
Let $E$ be the set of **Expected Options** (from the CSV `options` column) and $P$ be the set of **Predicted Options** extracted from the output.
An expected option $e \in E$ is matched if $e \in P$.

---

## 5. Scoring and Metric Definitions

All test cases are classified into mutually exclusive top-level categories, and then detailed option-level metrics are computed for successful tool selections.

### 5.1 Top-Level Metrics
* **Total Queries ($N$):** The total number of test cases.
* **Unsuccessful Retrievals (Errors):**
  * **Criteria:** Any execution that resulted in an Unsuccessful Retrieval (non-zero exit code, disambiguation message, or "Query unclear").
  * **Count ($N_{\text{error}}$):** Number of such runs.
  * **Percentage ($P_{\text{error}}$):** $\frac{N_{\text{error}}}{N} \times 100\%$
* **Complete Failure (Incorrect Tool Selected):**
  * **Criteria:** A command was successfully generated, but the Generated Tool does not match the Expected Tool.
  * **Count ($N_{\text{wrong\_tool}}$):** Number of such runs.
  * **Percentage ($P_{\text{wrong\_tool}}$):** $\frac{N_{\text{wrong\_tool}}}{N} \times 100\%$
* **Successful Tool Selection:**
  * **Criteria:** A command was successfully generated, and the Generated Tool matches the Expected Tool.
  * **Count ($N_{\text{correct\_tool}}$):** Number of such runs.
  * **Percentage ($P_{\text{correct\_tool}}$):** $\frac{N_{\text{correct\_tool}}}{N} \times 100\%$

### 5.2 Option-Level Metrics (For Correct Tool Selections)
For each test case $i$ where the correct tool was selected, we compute:
* Let $C_i = E_i \cap P_i$ be the set of correct options generated.
* **Option Recall Score ($S_i$):**
  $$S_i = \begin{cases}
     100\% & \text{if } |E_i| = 0 \\
     \frac{|C_i|}{|E_i|} \times 100\% & \text{if } |E_i| > 0
  \end{cases}$$

Using $S_i$, each correct tool run is categorized into one of three buckets:
1. **Full Option Match:**
   * **Criteria:** All expected options are present in the output ($S_i = 100\%$).
   * **Count ($N_{\text{full\_match}}$):** Number of such runs.
   * **Percentage of Total ($P_{\text{full\_match\_total}}$):** $\frac{N_{\text{full\_match}}}{N} \times 100\%$
   * **Percentage of Correct Tool ($P_{\text{full\_match\_correct}}$):** $\frac{N_{\text{full\_match}}}{N_{\text{correct\_tool}}} \times 100\%$
2. **Partial Option Match:**
   * **Criteria:** Some, but not all, expected options are present ($0\% < S_i < 100\%$).
   * **Count ($N_{\text{partial\_match}}$):** Number of such runs.
   * **Percentage of Total ($P_{\text{partial\_match\_total}}$):** $\frac{N_{\text{partial\_match}}}{N} \times 100\%$
   * **Percentage of Correct Tool ($P_{\text{partial\_match\_correct}}$):** $\frac{N_{\text{partial\_match}}}{N_{\text{correct\_tool}}} \times 100\%$
3. **No Option Match:**
   * **Criteria:** None of the expected options are present ($S_i = 0\%$, and $|E_i| > 0$).
   * **Count ($N_{\text{no\_match}}$):** Number of such runs.
   * **Percentage of Total ($P_{\text{no\_match\_total}}$):** $\frac{N_{\text{no\_match}}}{N} \times 100\%$
   * **Percentage of Correct Tool ($P_{\text{no\_match\_correct}}$):** $\frac{N_{\text{no\_match}}}{N_{\text{correct\_tool}}} \times 100\%$

#### Option Average Score
* **Average Option Recall:** The average recall score across all correct tool selections:
  $$\text{Average Option Recall} = \frac{1}{N_{\text{correct\_tool}}} \sum_{i \in \text{correct\_tool}} S_i$$

---

## 6. Edge Case Handling

1. **Empty Expected Options ($|E_i| = 0$):** If a test case requires no options, and the tool correctly retrieves the tool with no options, it is scored as a **Full Option Match ($100\%$ score)**. If it adds unexpected options, it is still counted as a Full Option Match because all *required* options (which is empty) are present.
2. **Symmetric Option Count evaluation:** To ensure we don't reward over-generation of irrelevant options, we will output the symmetric option count error (the [FLAGS/DEST](file:///home/sandbox-noadmin/RustroverProjects/command-finder/flags_dest_metric.md) metric) as secondary diagnostic information alongside the main recall metrics.
3. **Whitespace and Formatting Differences:** The parser must normalize spaces, strip quotes, and convert all parsed options to lowercase before comparisons.

---

## 7. Reporting format
The benchmark suite should produce two outputs:
1. **Summary Table:** A human-readable Markdown summary printout.
2. **Detailed Run Log (`benchmark_results.csv`):** A CSV file recording for each query:
   `ID, Raw Query, Expected Tool, Generated Tool, Expected Options, Generated Options, Tool Correctness, Option Recall Score (%), Status`
