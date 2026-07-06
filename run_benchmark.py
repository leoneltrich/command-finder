#!/usr/bin/env python3
import os
import csv
import json
import subprocess
import re
import sys

# Define file paths
CSV_PATH = "/home/sandbox-noadmin/RustroverProjects/command-finder/test_set_with_options.csv"
DB_PATH = "local_assistant.db"
BINARY_PATH = "./target/x86_64-unknown-linux-musl/release/command-finder"
OUTPUT_CSV_PATH = "/home/sandbox-noadmin/RustroverProjects/command-finder/benchmark_results.csv"

# ANSI Escape sequence regex
ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')

def strip_ansi(text):
    return ansi_escape.sub('', text)

def parse_expected_options(options_str):
    if not options_str or options_str.strip() == "":
        return set()
    try:
        opts = json.loads(options_str)
        if isinstance(opts, list):
            return set(str(o).strip().lower() for o in opts)
    except Exception:
        # Fallback in case of custom parsing need
        pass
    return set()

def extract_predicted_options(generated_command):
    # Split the command into tokens by whitespace
    tokens = generated_command.split()
    if not tokens:
        return set()
    
    # First token is the tool name (e.g. mv, cp), skip it
    option_tokens = tokens[1:]
    predicted = set()
    
    for token in option_tokens:
        token_lower = token.strip().lower()
        if token_lower.startswith("-m:"):
            predicted.add("-m")
        elif token_lower.startswith("--"):
            if "=" in token_lower:
                predicted.add(token_lower.split("=")[0])
            else:
                predicted.add(token_lower)
        elif token_lower.startswith("-") and not token_lower.startswith("--"):
            predicted.add(token_lower)
            
    return predicted

def extract_tool_from_disambiguation(raw_stdout):
    stripped = strip_ansi(raw_stdout).strip()
    for line in stripped.split('\n'):
        line = line.strip()
        if " - " in line:
            parts = line.split(" - ", 1)
            tool_candidate = parts[0].strip()
            if tool_candidate in ["mv", "cp", "ops-sync"]:
                return tool_candidate
    return ""

def calculate_flags_dest(expected_opts, predicted_opts):
    e_len = len(expected_opts)
    p_len = len(predicted_opts)
    if e_len > 0 and p_len > 0:
        ratio = p_len / e_len
        return max(ratio, 1.0 / ratio)
    elif e_len == 0 and p_len == 0:
        return 1.0
    else:
        return abs(e_len - p_len) + 1.0

def main():
    import argparse
    parser = argparse.ArgumentParser(description="Run technical benchmark on command-finder.")
    parser.add_argument("-l", "--limit", type=int, default=None, help="Limit the number of queries to run for quick verification.")
    parser.add_argument("-o", "--output", type=str, default=OUTPUT_CSV_PATH, help="Path to output CSV results.")
    args = parser.parse_args()

    # Verify binary exists
    if not os.path.exists(BINARY_PATH):
        print(f"Error: Binary not found at {BINARY_PATH}. Please compile first or verify target architecture.")
        sys.exit(1)

    # Verify input CSV exists
    if not os.path.exists(CSV_PATH):
        print(f"Error: Test CSV not found at {CSV_PATH}.")
        sys.exit(1)

    # Load test dataset
    rows = []
    with open(CSV_PATH, mode='r', encoding='utf-8') as f:
        reader = csv.DictReader(f)
        for row in reader:
            rows.append(row)

    if args.limit is not None:
        rows = rows[:args.limit]

    total_queries = len(rows)
    print(f"Loaded {total_queries} test queries from CSV.")
    print("Starting benchmark execution. Running sequentially to prevent SQLite contention...")

    # Environment variables
    env = os.environ.copy()
    env["DATABASE_PATH"] = DB_PATH

    results = []

    # Counters
    n_error = 0
    n_wrong_tool = 0
    n_correct_tool = 0

    # Option categories (among correct tool matches)
    n_full_match = 0
    n_partial_match = 0
    n_no_match = 0

    option_recall_scores = []
    flags_dest_scores = []

    for idx, row in enumerate(rows, start=1):
        query_id = row.get("ID", f"unknown-{idx}")
        raw_query = row.get("Original Text", "")
        ground_truth_cmd = row.get("Category", "").strip()
        expected_options_raw = row.get("options", "").strip()

        # Parse expected tool (first word of ground truth command)
        expected_tool = ground_truth_cmd.split()[0] if ground_truth_cmd else ""
        expected_opts = parse_expected_options(expected_options_raw)

        # Execute command-finder
        try:
            res = subprocess.run(
                [BINARY_PATH, "query", raw_query],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=10 # 10 seconds safety timeout per query
            )
            raw_stdout = res.stdout
            raw_stderr = res.stderr
            return_code = res.returncode
        except subprocess.TimeoutExpired:
            raw_stdout = ""
            raw_stderr = "Timeout expired"
            return_code = -1
        except Exception as e:
            raw_stdout = ""
            raw_stderr = str(e)
            return_code = -2

        stripped_stdout = strip_ansi(raw_stdout).strip()
        stripped_stderr = strip_ansi(raw_stderr).strip()

        # Classification of errors
        is_error = False
        error_reason = ""
        generated_command = ""
        generated_tool = ""
        predicted_opts = set()
        recall_score = 0.0
        flags_dest = 1.0
        tool_correctness = "N/A"

        if return_code != 0:
            is_error = True
            error_reason = f"Process exit code {return_code}"
        elif "The query could not be resolved" in stripped_stdout:
            is_error = True
            error_reason = "Disambiguation warning"
        elif stripped_stdout == "Query unclear":
            is_error = True
            error_reason = "Query unclear"
        elif stripped_stderr:
            is_error = True
            error_reason = f"Stderr error: {stripped_stderr[:100]}"

        if is_error:
            n_error += 1
            status = f"Error: {error_reason}"
            if error_reason == "Disambiguation warning":
                generated_tool = extract_tool_from_disambiguation(raw_stdout)
                tool_correctness = "Yes" if generated_tool.lower() == expected_tool.lower() else "No"
            else:
                tool_correctness = "N/A"
        else:
            # Successful retrieval -> extract command
            # Try matching green block first
            match = re.search(r'\x1b\[1;32m(.*?)\x1b\[0m', raw_stdout, re.IGNORECASE | re.DOTALL)
            if match:
                generated_command = match.group(1).strip()
            else:
                # Fallback to the first line of stripped stdout
                lines = [l.strip() for l in stripped_stdout.split('\n') if l.strip()]
                if lines:
                    generated_command = lines[0]

            if generated_command:
                generated_tool = generated_command.split()[0]
            else:
                generated_tool = ""

            # Check if correct tool
            if generated_tool.lower() != expected_tool.lower():
                n_wrong_tool += 1
                status = "Incorrect Tool Selected"
            else:
                n_correct_tool += 1
                predicted_opts = extract_predicted_options(generated_command)
                
                # Check option matches
                correct_opts = expected_opts.intersection(predicted_opts)
                if len(expected_opts) == 0:
                    recall_score = 100.0
                else:
                    recall_score = (len(correct_opts) / len(expected_opts)) * 100.0

                flags_dest = calculate_flags_dest(expected_opts, predicted_opts)
                option_recall_scores.append(recall_score)
                flags_dest_scores.append(flags_dest)

                if recall_score == 100.0:
                    n_full_match += 1
                    status = "Full Option Match"
                elif len(correct_opts) == 0 and len(expected_opts) > 0:
                    n_no_match += 1
                    status = "No Option Match"
                else:
                    n_partial_match += 1
                    status = f"Partial Option Match ({recall_score:.1f}%)"

        # Record detail
        results.append({
            "ID": query_id,
            "Raw Query": raw_query,
            "Expected Tool": expected_tool,
            "Generated Tool": generated_tool,
            "Expected Options": sorted(list(expected_opts)),
            "Generated Options": sorted(list(predicted_opts)),
            "Tool Correctness": tool_correctness if is_error else ("Yes" if generated_tool.lower() == expected_tool.lower() else ("No" if generated_tool else "N/A")),
            "Option Recall Score (%)": f"{recall_score:.2f}" if not is_error and generated_tool.lower() == expected_tool.lower() else "N/A",
            "FLAGS/DEST Symmetric Error": f"{flags_dest:.2f}" if not is_error and generated_tool.lower() == expected_tool.lower() else "N/A",
            "Status": status
        })

        if idx % 50 == 0 or idx == total_queries:
            print(f"Processed {idx}/{total_queries} queries...")

    # Write detailed CSV log
    with open(args.output, mode='w', encoding='utf-8', newline='') as f:
        fieldnames = ["ID", "Raw Query", "Expected Tool", "Generated Tool", "Expected Options", "Generated Options", "Tool Correctness", "Option Recall Score (%)", "FLAGS/DEST Symmetric Error", "Status"]
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(results)

    # Compute aggregate percentages
    p_error = (n_error / total_queries) * 100.0
    p_wrong_tool = (n_wrong_tool / total_queries) * 100.0
    p_correct_tool = (n_correct_tool / total_queries) * 100.0

    avg_recall = sum(option_recall_scores) / len(option_recall_scores) if option_recall_scores else 0.0
    avg_flags_dest = sum(flags_dest_scores) / len(flags_dest_scores) if flags_dest_scores else 1.0

    # Print summary report
    print("\n" + "="*50)
    print("TECHNICAL BENCHMARK SUMMARY REPORT")
    print("="*50)
    print(f"Total Test Queries (N):                   {total_queries}")
    print(f"Unsuccessful Retrievals (Errors):         {n_error} ({p_error:.2f}%)")
    print(f"Complete Failures (Incorrect Tool):       {n_wrong_tool} ({p_wrong_tool:.2f}%)")
    print(f"Successful Tool Selection:                {n_correct_tool} ({p_correct_tool:.2f}%)")
    print("-"*50)
    print(f"Within Successful Tool Selections (N_correct = {n_correct_tool}):")
    if n_correct_tool > 0:
        p_full_tot = (n_full_match / total_queries) * 100.0
        p_full_corr = (n_full_match / n_correct_tool) * 100.0
        p_part_tot = (n_partial_match / total_queries) * 100.0
        p_part_corr = (n_partial_match / n_correct_tool) * 100.0
        p_no_tot = (n_no_match / total_queries) * 100.0
        p_no_corr = (n_no_match / n_correct_tool) * 100.0

        print(f"  Full Option Matches:                  {n_full_match} (of total: {p_full_tot:.2f}%, of correct: {p_full_corr:.2f}%)")
        print(f"  Partial Option Matches:               {n_partial_match} (of total: {p_part_tot:.2f}%, of correct: {p_part_corr:.2f}%)")
        print(f"  No Option Matches:                    {n_no_match} (of total: {p_no_tot:.2f}%, of correct: {p_no_corr:.2f}%)")
        print(f"  Average Option Recall Score:          {avg_recall:.2f}%")
        print(f"  Average FLAGS/DEST Symmetric Error:   {avg_flags_dest:.2f}")
    else:
        print("  No correct tool selections to report option statistics.")
    print("="*50)
    print(f"Detailed run log exported to: {args.output}\n")

if __name__ == "__main__":
    main()
