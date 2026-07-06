#!/usr/bin/env python3
import os
import csv
import sys

CSV_PATH = "/home/sandbox-noadmin/RustroverProjects/command-finder/benchmark_results.csv"

def main():
    if not os.path.exists(CSV_PATH):
        print(f"Error: Results CSV not found at {CSV_PATH}.")
        print("Please run the benchmark script first: ./run_benchmark.py")
        sys.exit(1)

    all_rows = []
    with open(CSV_PATH, mode='r', encoding='utf-8') as f:
        reader = csv.DictReader(f)
        for row in reader:
            all_rows.append(row)

    total_queries = len(all_rows)
    if total_queries == 0:
        print("Error: The results CSV file is empty.")
        sys.exit(1)

    # Filter rows where the tool was correctly selected
    correct_tool_rows = []
    incorrect_tool_rows = []
    error_rows = []
    for row in all_rows:
        correctness = row.get("Tool Correctness", "").strip().lower()
        if correctness == "yes":
            correct_tool_rows.append(row)
        elif correctness == "no":
            incorrect_tool_rows.append(row)
        else:
            error_rows.append(row)

    n_correct = len(correct_tool_rows)
    n_incorrect = len(incorrect_tool_rows)
    n_errors = len(error_rows)

    # Option categorization for correct tool rows
    fully_correct = []
    partially_correct = []
    non_correct = []

    for row in correct_tool_rows:
        score_str = row.get("Option Recall Score (%)", "N/A").strip()
        if score_str == "N/A":
            continue
        try:
            score = float(score_str)
        except ValueError:
            continue

        if score == 100.0:
            fully_correct.append(score)
        elif score == 0.0:
            non_correct.append(score)
        else:
            partially_correct.append(score)

    n_full = len(fully_correct)
    n_partial = len(partially_correct)
    n_none = len(non_correct)

    p_full_tot = (n_full / total_queries) * 100.0
    p_partial_tot = (n_partial / total_queries) * 100.0
    p_none_tot = (n_none / total_queries) * 100.0

    p_full_corr = (n_full / n_correct) * 100.0 if n_correct > 0 else 0.0
    p_partial_corr = (n_partial / n_correct) * 100.0 if n_correct > 0 else 0.0
    p_none_corr = (n_none / n_correct) * 100.0 if n_correct > 0 else 0.0

    avg_recall_full = sum(fully_correct) / n_full if n_full > 0 else 0.0
    avg_recall_partial = sum(partially_correct) / n_partial if n_partial > 0 else 0.0
    avg_recall_none = sum(non_correct) / n_none if n_none > 0 else 0.0
    
    all_correct_scores = fully_correct + partially_correct + non_correct
    avg_recall_overall = sum(all_correct_scores) / len(all_correct_scores) if all_correct_scores else 0.0

    # Incorrect tool analysis
    incorrect_picks = {}  # How many times was a category generated/picked incorrectly
    missed_expected = {}  # How many times was a category expected but missed
    
    for row in incorrect_tool_rows:
        gen_tool = row.get("Generated Tool", "").strip().lower()
        exp_tool = row.get("Expected Tool", "").strip().lower()
        if gen_tool:
            incorrect_picks[gen_tool] = incorrect_picks.get(gen_tool, 0) + 1
        if exp_tool:
            missed_expected[exp_tool] = missed_expected.get(exp_tool, 0) + 1

    # Get a list of all unique tools in dataset
    all_tools = sorted(list(set(
        [r.get("Expected Tool", "").strip().lower() for r in all_rows if r.get("Expected Tool", "").strip()] +
        [r.get("Generated Tool", "").strip().lower() for r in all_rows if r.get("Generated Tool", "").strip()]
    )))

    # Print Option Report Table
    print("\n" + "="*85)
    print("                      DETAILED OPTION ACCURACY ANALYTICS REPORT")
    print("="*85)
    print(f"Total Test Queries (N): {total_queries:<5}  |  Correct Tool Selections (N_correct): {n_correct}")
    print("-"*85)
    
    # Table headers
    print(f"{'Option Match Subcategory':<25} | {'Count':<8} | {'% of Total (N)':<16} | {'% of Correct':<14} | {'Avg Option Recall':<17}")
    print("-"*85)
    
    print(f"{'Fully Correct (100% Match)':<25} | {n_full:<8} | {p_full_tot:>13.2f}% | {p_full_corr:>11.2f}% | {avg_recall_full:>14.2f}%")
    print(f"{'Partially Correct (1-99%)':<25} | {n_partial:<8} | {p_partial_tot:>13.2f}% | {p_partial_corr:>11.2f}% | {avg_recall_partial:>14.2f}%")
    print(f"{'Non-Correct (0% Match)':<25} | {n_none:<8} | {p_none_tot:>13.2f}% | {p_none_corr:>11.2f}% | {avg_recall_none:>14.2f}%")
    print("-"*85)

    # Print Summary Averages
    print("\n" + "="*85)
    print("                               SUMMARY AVERAGES")
    print("="*85)
    print(f"1. Overall Option Recall (Full, Partial, None):                  {avg_recall_overall:.2f}%")
    print(f"   (Average % of correct options generated across all correct tool selections)")
    print()
    print(f"2. Partial Match Option Recall (Partial Only):                   {avg_recall_partial:.2f}%")
    print(f"   (Average % of correct options generated only for partially matched cases)")
    print("="*85)

    # Print Incorrect Tool Analytics Report
    print("\n" + "="*85)
    print("                    INCORRECT TOOL SELECTION ANALYTICS REPORT")
    print("="*85)
    print(f"Total Incorrect Tool Selections (N_incorrect): {n_incorrect} ({(n_incorrect/total_queries)*100.0:.2f}% of total test set)")
    print("-"*85)
    
    # Table: Incorrectly Picked Categories
    print(f"A. CATEGORIES INCORRECTLY GENERATED (Picked when they should NOT have been)")
    print("-"*85)
    print(f"{'Tool / Category Name':<25} | {'Count':<8} | {'% of Total (N)':<16} | {'% of Incorrect (N_incorrect)':<25}")
    print("-"*85)
    
    for tool in all_tools:
        count = incorrect_picks.get(tool, 0)
        p_tot = (count / total_queries) * 100.0
        p_inc = (count / n_incorrect) * 100.0 if n_incorrect > 0 else 0.0
        print(f"{tool:<25} | {count:<8} | {p_tot:>13.2f}% | {p_inc:>23.2f}%")
    print("-"*85)
    print()

    # Table: Missed expected tools
    print(f"B. EXPECTED CATEGORIES MISSED (Should have been picked but resolved wrongly)")
    print("-"*85)
    print(f"{'Tool / Category Name':<25} | {'Count':<8} | {'% of Total (N)':<16} | {'% of Incorrect (N_incorrect)':<25}")
    print("-"*85)
    
    for tool in all_tools:
        count = missed_expected.get(tool, 0)
        p_tot = (count / total_queries) * 100.0
        p_inc = (count / n_incorrect) * 100.0 if n_incorrect > 0 else 0.0
        print(f"{tool:<25} | {count:<8} | {p_tot:>13.2f}% | {p_inc:>23.2f}%")
    print("="*85)

    # Print Unresolved Queries / Error Report
    unresolved_reasons = {}
    for row in error_rows:
        status = row.get("Status", "").strip()
        reason = status[7:] if status.startswith("Error: ") else status
        unresolved_reasons[reason] = unresolved_reasons.get(reason, 0) + 1

    print("\n" + "="*85)
    print("                      UNRESOLVED QUERIES / ERRORS REPORT")
    print("="*85)
    print(f"Total Unresolved / Errors (N_errors): {n_errors} ({(n_errors/total_queries)*100.0:.2f}% of total test set)")
    print("-"*85)
    print(f"{'Reason / Error Mode':<40} | {'Count':<8} | {'% of Total (N)':<16} | {'% of Errors':<14}")
    print("-"*85)
    for reason, count in sorted(unresolved_reasons.items(), key=lambda x: x[1], reverse=True):
        p_tot = (count / total_queries) * 100.0
        p_err = (count / n_errors) * 100.0 if n_errors > 0 else 0.0
        print(f"{reason:<40} | {count:<8} | {p_tot:>13.2f}% | {p_err:>11.2f}%")
    print("="*85 + "\n")

if __name__ == "__main__":
    main()
