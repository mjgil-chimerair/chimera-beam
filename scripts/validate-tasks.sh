#!/bin/bash
# validate-tasks.sh - Validate task completion evidence
# Usage: ./scripts/validate-tasks.sh [--task N] [--crate NAME]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TASK_LIST="$PROJECT_ROOT/docs/task-list-3.md"
VALIDATION_ERRORS=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

function log_error() {
    echo -e "${RED}ERROR: $1${NC}"
    ((VALIDATION_ERRORS++))
}

function log_warn() {
    echo -e "${YELLOW}WARN: $1${NC}"
}

function log_success() {
    echo -e "${GREEN}OK: $1${NC}"
}

function check_implementation() {
    local task_num=$1
    local crate=$2
    echo "Checking implementation for Task $task_num in $crate..."
    
    # Check for unwrap/panic in crate source
    local unwrap_count
    unwrap_count=$(grep -rn "unwrap()\|expect(\"\|panic!" "$PROJECT_ROOT/crates/$crate/src/" 2>/dev/null | wc -l)
    
    if [ "$unwrap_count" -gt 0 ]; then
        log_warn "Found $unwrap_count unwrap/panic calls in $crate"
    else
        log_success "No unwrap/panic found in $crate"
    fi
}

function check_tests() {
    local crate=$1
    echo "Checking tests for $crate..."
    
    # Count test functions
    local test_count
    test_count=$(grep -rn "#\[test\]" "$PROJECT_ROOT/crates/$crate/src/" 2>/dev/null | wc -l)
    
    if [ "$test_count" -gt 0 ]; then
        log_success "Found $test_count tests in $crate"
    else
        log_warn "No tests found in $crate"
    fi
}

function check_docs() {
    local crate=$1
    echo "Checking documentation for $crate..."
    
    # Try to build docs and check for missing doc warnings
    local doc_output
    doc_output=$(cd "$PROJECT_ROOT" && cargo doc -p "$crate" 2>&1 || true)
    
    local missing_doc_count
    missing_doc_count=$(echo "$doc_output" | grep -c "missing documentation" || echo "0")
    
    if [ "$missing_doc_count" -eq 0 ]; then
        log_success "No missing documentation in $crate"
    else
        log_error "Found $missing_doc_count missing documentation warnings in $crate"
    fi
}

function validate_task() {
    local task_num=$1
    echo "=========================================="
    echo "Validating Task $task_num"
    echo "=========================================="
    
    # Extract task info from task list
    # This is a simplified check - in practice, you'd parse the markdown more carefully
    local task_status
    task_status=$(grep -A 1 "### Task $task_num:" "$TASK_LIST" | grep "Status" | grep -oP 'Incomplete|In Progress|Complete' || echo "Unknown")
    
    if [ "$task_status" = "Complete" ]; then
        echo "Task $task_num is marked Complete - running full validation..."
        # For now, just check all crates
        for crate in rustzigbeam_core rustzigbeam_abi rustzigbeam_term rustzigbeam_heap rustzigbeam_process rustzigbeam_instr rustzigbeam_bif rustzigbeam_scheduler rustzigbeam_vm rustzigbeam_code rustzigbeam_dist rustzigbeam_timer rustzigbeam_runtime; do
            if [ -d "$PROJECT_ROOT/crates/$crate" ]; then
                check_implementation "$task_num" "$crate"
                check_tests "$crate"
                check_docs "$crate"
            fi
        done
    else
        log_warn "Task $task_num status is '$task_status' - skipping validation"
    fi
}

# Main logic
if [ "$1" = "--task" ]; then
    validate_task "$2"
elif [ "$1" = "--crate" ]; then
    check_implementation "N/A" "$2"
    check_tests "$2"
    check_docs "$2"
else
    echo "Usage:"
    echo "  $0 --task N       # Validate specific task"
    echo "  $0 --crate NAME   # Validate specific crate"
    echo ""
    echo "Running full validation..."
    
    # Validate all tasks marked Complete
    for task_num in $(grep -oP '(?<=### Task )\d+' "$TASK_LIST" 2>/dev/null || echo ""); do
        if [ -n "$task_num" ]; then
            validate_task "$task_num"
        fi
    done
fi

echo ""
echo "=========================================="
if [ "$VALIDATION_ERRORS" -eq 0 ]; then
    log_success "All validations passed!"
    exit 0
else
    log_error "Found $VALIDATION_ERRORS validation error(s)"
    exit 1
fi
