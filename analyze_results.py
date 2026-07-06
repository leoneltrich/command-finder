#!/usr/bin/env python3
import os
import csv
import sys

CSV_PATH = "/home/sandbox-noadmin/RustroverProjects/command-finder/benchmark_results.csv"

def main():
    import argparse
    parser = argparse.ArgumentParser(description="Analyze benchmark results.")
    parser.add_argument("-w", "--wrong-tools", action="store_true", help="Print details of test cases where the incorrect tool was selected.")
    args = parser.parse_args()

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
        status = row.get("Status", "").strip()
        if status.startswith("Error:"):
            error_rows.append(row)
        else:
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
    fully_correct_recall = []
    fully_correct_fd = []
    partially_correct_recall = []
    partially_correct_fd = []
    non_correct_recall = []
    non_correct_fd = []
    overall_fd = []

    for row in correct_tool_rows:
        score_str = row.get("Option Recall Score (%)", "N/A").strip()
        if score_str == "N/A":
            continue
        try:
            score = float(score_str)
        except ValueError:
            continue

        fd_str = row.get("FLAGS/DEST Symmetric Error", "N/A").strip()
        fd = None
        if fd_str != "N/A":
            try:
                fd = float(fd_str)
                overall_fd.append(fd)
            except ValueError:
                pass

        if score == 100.0:
            fully_correct_recall.append(score)
            if fd is not None:
                fully_correct_fd.append(fd)
        elif score == 0.0:
            non_correct_recall.append(score)
            if fd is not None:
                non_correct_fd.append(fd)
        else:
            partially_correct_recall.append(score)
            if fd is not None:
                partially_correct_fd.append(fd)

    n_full = len(fully_correct_recall)
    n_partial = len(partially_correct_recall)
    n_none = len(non_correct_recall)

    p_full_tot = (n_full / total_queries) * 100.0
    p_partial_tot = (n_partial / total_queries) * 100.0
    p_none_tot = (n_none / total_queries) * 100.0

    p_full_corr = (n_full / n_correct) * 100.0 if n_correct > 0 else 0.0
    p_partial_corr = (n_partial / n_correct) * 100.0 if n_correct > 0 else 0.0
    p_none_corr = (n_none / n_correct) * 100.0 if n_correct > 0 else 0.0

    avg_recall_full = sum(fully_correct_recall) / n_full if n_full > 0 else 0.0
    avg_recall_partial = sum(partially_correct_recall) / n_partial if n_partial > 0 else 0.0
    avg_recall_none = sum(non_correct_recall) / n_none if n_none > 0 else 0.0
    
    avg_recall_overall = sum(fully_correct_recall + partially_correct_recall + non_correct_recall) / n_correct if n_correct > 0 else 0.0

    avg_fd_full = sum(fully_correct_fd) / len(fully_correct_fd) if fully_correct_fd else 1.0
    avg_fd_partial = sum(partially_correct_fd) / len(partially_correct_fd) if partially_correct_fd else 1.0
    avg_fd_none = sum(non_correct_fd) / len(non_correct_fd) if non_correct_fd else 1.0
    avg_fd_overall = sum(overall_fd) / len(overall_fd) if overall_fd else 1.0

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
    print("\n" + "="*100)
    print("                      DETAILED OPTION ACCURACY ANALYTICS REPORT")
    print("="*100)
    print(f"Total Test Queries (N): {total_queries:<5}  |  Correct Tool Selections (N_correct): {n_correct}")
    print("-"*100)
    
    # Table headers
    print(f"{'Option Match Subcategory':<25} | {'Count':<8} | {'% of Total (N)':<16} | {'% of Correct':<14} | {'Avg Option Recall':<17} | {'Avg FLAGS/DEST':<15}")
    print("-"*100)
    
    print(f"{'Fully Correct (100% Match)':<25} | {n_full:<8} | {p_full_tot:>13.2f}% | {p_full_corr:>11.2f}% | {avg_recall_full:>14.2f}% | {avg_fd_full:>13.2f}")
    print(f"{'Partially Correct (1-99%)':<25} | {n_partial:<8} | {p_partial_tot:>13.2f}% | {p_partial_corr:>11.2f}% | {avg_recall_partial:>14.2f}% | {avg_fd_partial:>13.2f}")
    print(f"{'Non-Correct (0% Match)':<25} | {n_none:<8} | {p_none_tot:>13.2f}% | {p_none_corr:>11.2f}% | {avg_recall_none:>14.2f}% | {avg_fd_none:>13.2f}")
    print("-"*100)

    # Print Summary Averages
    print("\n" + "="*100)
    print("                               SUMMARY AVERAGES")
    print("="*100)
    print(f"1. Overall Option Recall (Full, Partial, None):                  {avg_recall_overall:.2f}%")
    print(f"   (Average % of correct options generated across all correct tool selections)")
    print()
    print(f"2. Partial Match Option Recall (Partial Only):                   {avg_recall_partial:.2f}%")
    print(f"   (Average % of correct options generated only for partially matched cases)")
    print()
    print(f"3. Overall FLAGS/DEST Symmetric Error:                           {avg_fd_overall:.2f}")
    print(f"   (Average option count error across all correct tool selections; 1.00 is perfect)")
    print()
    print(f"4. Fully Correct FLAGS/DEST Symmetric Error:                     {avg_fd_full:.2f}")
    print(f"   (Average option count error for 100% option matches)")
    print()
    print(f"5. Partially Correct FLAGS/DEST Symmetric Error:                  {avg_fd_partial:.2f}")
    print(f"   (Average option count error for 1-99% option matches)")
    print()
    print(f"6. Non-Correct FLAGS/DEST Symmetric Error:                       {avg_fd_none:.2f}")
    print(f"   (Average option count error for 0% option matches)")
    print("="*100)

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
    n_err_correct = 0
    n_err_wrong = 0
    n_err_na = 0

    for row in error_rows:
        status = row.get("Status", "").strip()
        reason = status[7:] if status.startswith("Error: ") else status
        unresolved_reasons[reason] = unresolved_reasons.get(reason, 0) + 1

        correctness = row.get("Tool Correctness", "").strip().lower()
        if correctness == "yes":
            n_err_correct += 1
        elif correctness == "no":
            n_err_wrong += 1
        else:
            n_err_na += 1

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
    print("-"*85)
    
    # Tool Selection correctness for resolution failures
    p_err_correct = (n_err_correct / n_errors) * 100.0 if n_errors > 0 else 0.0
    p_err_wrong = (n_err_wrong / n_errors) * 100.0 if n_errors > 0 else 0.0
    p_err_na = (n_err_na / n_errors) * 100.0 if n_errors > 0 else 0.0

    print(f"Tool Selection Accuracy for Resolution Failures (N_errors = {n_errors}):")
    print(f"  - Correct Tool identified before failing:  {n_err_correct:>4} ({p_err_correct:>6.2f}%)")
    print(f"  - Incorrect Tool identified before failing: {n_err_wrong:>4} ({p_err_wrong:>6.2f}%)")
    print(f"  - No Tool identified (e.g. Unclear / Crash): {n_err_na:>4} ({p_err_na:>6.2f}%)")
    print("="*85 + "\n")

    # Optionally print detailed incorrect tool selection cases
    if args.wrong_tools:
        print("="*100)
        print("                  DETAILED LIST OF INCORRECT TOOL SELECTIONS")
        print("="*100)
        print(f"{'ID':<8} | {'Expected':<10} | {'Generated':<10} | {'Raw Query'}")
        print("-"*100)
        for row in incorrect_tool_rows:
            query_id = row.get("ID", "").strip()
            exp_tool = row.get("Expected Tool", "").strip()
            gen_tool = row.get("Generated Tool", "").strip()
            raw_query = row.get("Raw Query", "").strip()
            print(f"{query_id:<8} | {exp_tool:<10} | {gen_tool:<10} | {raw_query}")
        print("="*100 + "\n")

if __name__ == "__main__":
    main()
