---
title: Security Vulnerability Found in Dependencies
labels: security, dependencies
assignees: ryuichi1208
---

## Security Audit Alert

A security vulnerability has been detected in one or more of the project dependencies by the weekly security audit.

### Details

The automated security audit performed on `{{ date | date('dddd, MMMM Do YYYY, h:mm:ss a') }}` has found vulnerabilities in the project dependencies.

Please check the GitHub Actions log for complete details of the vulnerabilities:
{{ env.GITHUB_SERVER_URL }}/{{ env.GITHUB_REPOSITORY }}/actions/runs/{{ env.GITHUB_RUN_ID }}

### Action Required

1. Review the security vulnerabilities in the GitHub Actions log
2. Update the affected dependencies to versions without the vulnerabilities
3. Verify that the updates don't introduce breaking changes
4. Run `cargo audit` locally to ensure all vulnerabilities are addressed

### Importance

Keeping dependencies free of known security vulnerabilities is critical for the security of this module and any systems that use it.

This issue was automatically created by the GitHub Actions security audit workflow.
