---
id: TASK-138
title: >-
  Security Penetration Testing - Verify Route Authorization, Secret Management,
  and Attack Surface
status: Backlog
assignee: []
created_date: '2026-02-28 02:44'
labels:
  - security
  - testing
  - pentesting
  - pre-release
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement comprehensive security penetration testing to validate that all API routes are properly secured, secrets are safely managed, and the system is resilient against common attack vectors before the UI release.

## Context

Before merging dev into main and releasing the UI, we need confidence that:
- All API routes enforce proper authentication and authorization
- Secrets (API keys, database credentials, signing keys) are not exposed
- The system is resilient against common web application attacks
- Agent-to-server communication is secure
- RBAC policies are correctly enforced

## Current Security Posture

We have:
- OIDC authentication with JIT provisioning
- Role-based authorization (Admin, Operator, Viewer)
- API key authentication for agents
- Public key cryptography for agent identity

But we need to verify these work correctly under adversarial conditions.

## Proposed Approach

Create a NixOS VM test with three nodes:
1. **Server Node** - Running crystal-forge-server with full configuration
2. **Agent Node** - Running crystal-forge-agent with valid credentials
3. **Threat Actor Node** - Simulating various attack scenarios

The threat actor node should attempt:
- Unauthenticated API access
- Privilege escalation (Viewer → Operator → Admin)
- Secret extraction (environment variables, config files, logs)
- Agent impersonation
- CSRF/XSS attacks on UI endpoints
- SQL injection on API endpoints
- Path traversal attacks
- Rate limiting bypass
- Session hijacking

## Test Categories

### 1. Authentication Bypass
- Access protected routes without auth token
- Use expired/invalid JWT tokens
- Attempt to forge JWT signatures
- Test OIDC callback manipulation

### 2. Authorization Escalation
- Viewer role attempting Operator/Admin actions
- Cross-environment access (access systems in other environments)
- Bypass RBAC checks with malformed requests

### 3. Secret Exposure
- Check for secrets in:
  - HTTP response headers
  - Error messages
  - Server logs
  - API responses
  - JavaScript bundles
  - Environment variable leakage
- Verify secrets are masked in GitLab CI logs

### 4. Agent Security
- Attempt to register rogue agent
- Replay agent authentication requests
- MITM agent-server communication
- Agent key extraction attempts

### 5. Input Validation
- SQL injection on all API endpoints
- Command injection (system names, flake URLs, etc.)
- Path traversal in file operations
- XXE attacks on any XML input
- JSON injection

### 6. Session Security
- Session fixation
- Session hijacking
- CSRF token bypass
- Cookie manipulation

### 7. Rate Limiting & DoS
- Brute force authentication
- API endpoint flooding
- Resource exhaustion attacks

## Implementation Plan

### Phase 1: Test Infrastructure
- Create `checks/security-pentest/default.nix`
- Define three-node NixOS test setup
- Configure threat actor node with attack tools (nmap, sqlmap, burp suite CLI, etc.)

### Phase 2: Attack Scenarios
- Write Python test cases for each attack category
- Use pytest-bdd for behavior-driven security tests
- Generate attack reports in SARIF format for GitLab

### Phase 3: Automated Detection
- All tests should FAIL if vulnerabilities are found
- Block merge to main if any HIGH severity issues detected
- Create GitLab Security Dashboard integration

### Phase 4: Documentation
- Document security testing methodology
- Create runbook for security incident response
- Document secure deployment practices

## Example Test Structure

```python
@pytest.mark.security
@pytest.mark.high_severity
def test_unauthenticated_api_access_denied(threat_actor, server):
    """Verify all API routes require authentication"""
    
    # Attempt to access protected endpoints without auth
    response = threat_actor.succeed(
        "curl -s -o /dev/null -w '%{http_code}' http://server:3000/api/systems"
    )
    
    assert response == "401", "Unauthenticated access should be denied"
    
@pytest.mark.security  
@pytest.mark.critical
def test_viewer_cannot_create_systems(threat_actor, server):
    """Verify Viewer role cannot perform write operations"""
    
    viewer_token = get_viewer_jwt_token(server)
    
    response = threat_actor.succeed(
        f"curl -X POST -H 'Authorization: Bearer {viewer_token}' "
        "-H 'Content-Type: application/json' "
        "-d '{\"hostname\":\"evil\"}' "
        "http://server:3000/api/systems"
    )
    
    assert "403" in response, "Viewer should not be able to create systems"
```

## Success Criteria

- All authentication bypass attempts fail
- All authorization escalation attempts fail
- No secrets exposed in any channel
- Agent impersonation attempts fail
- All input validation works correctly
- Rate limiting prevents brute force
- Session security is enforced

## Out of Scope

- Physical security testing
- Social engineering attacks
- DDoS mitigation (we're not a cloud service yet)
- Third-party dependency audits (separate task)

## References

- OWASP Top 10: https://owasp.org/www-project-top-ten/
- OWASP API Security: https://owasp.org/www-project-api-security/
- CWE Top 25: https://cwe.mitre.org/top25/
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Three-node NixOS test exists (server, agent, threat-actor)
- [ ] #2 Threat actor node has penetration testing tools installed
- [ ] #3 Test suite covers all 7 attack categories listed above
- [ ] #4 All authentication bypass attempts are blocked
- [ ] #5 All authorization escalation attempts are blocked
- [ ] #6 No secrets are exposed in logs, responses, or environment
- [ ] #7 Agent impersonation attempts fail
- [ ] #8 SQL injection attempts are blocked on all endpoints
- [ ] #9 CSRF protection works on all state-changing operations
- [ ] #10 Test generates SARIF report for GitLab Security Dashboard
- [ ] #11 CI job fails if any HIGH severity vulnerabilities found
- [ ] #12 Documentation includes security testing runbook
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Security test suite runs in CI as 'security-pentest' check
- [ ] #2 All existing tests pass without vulnerabilities
- [ ] #3 Security findings documented and filed as separate tasks if needed
<!-- DOD:END -->
