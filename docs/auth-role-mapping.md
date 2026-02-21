# OIDC Role Mapping Configuration

Crystal Forge maps OIDC groups to local RBAC roles on every login.

## Roles

Three roles are supported:
- **Admin**: Full system access
- **Operator**: Can manage deployments and builds
- **Viewer**: Read-only access

## Configuration

### Environment Variables

**`CRYSTAL_FORGE_ROLE_MAPPING`** (strongly recommended)  
JSON object mapping OIDC group names to Crystal Forge roles.

Example:
```bash
export CRYSTAL_FORGE_ROLE_MAPPING='{"crystal-forge-admins":"admin","crystal-forge-operators":"operator","crystal-forge-viewers":"viewer"}'
```

⚠️ **If JSON parsing fails**, the server will log an error and effectively lock everyone out unless `CRYSTAL_FORGE_DEFAULT_ROLE` is set.

**`CRYSTAL_FORGE_DEFAULT_ROLE`** (optional)  
Default role assigned when user's OIDC groups don't match any mapping.

Example:
```bash
export CRYSTAL_FORGE_DEFAULT_ROLE="viewer"
```

**Default behavior (production):**  
If both `CRYSTAL_FORGE_ROLE_MAPPING` and `CRYSTAL_FORGE_DEFAULT_ROLE` are unset (or JSON parsing fails), **all OIDC logins will be denied** (safe-deny). The server will log a warning at startup.

### Group Claim Source

**`CRYSTAL_FORGE_OIDC_ROLES_CLAIM`** (optional, default: `groups`)  
OIDC claim containing group/role information.

Example:
```bash
export CRYSTAL_FORGE_OIDC_ROLES_CLAIM="roles"  # Use "roles" instead of "groups"
```

## Role Selection Logic

When a user has multiple matching groups, the **highest privilege role** is assigned:

1. Admin (highest)
2. Operator
3. Viewer (lowest)

Example:
```json
{
  "engineering": "operator",
  "leadership": "admin"
}
```

User in both `engineering` and `leadership` groups → assigned **Admin** role.

## Role Synchronization

Roles are synchronized **on every login**:
- Old role assignments are removed
- New role is assigned based on current OIDC groups
- Changes take effect immediately on next login

## Safe-Deny Behavior

If no default role is configured and the user's groups don't match:
- Login is **rejected**
- HTTP 403 Forbidden: "No matching role for OIDC groups - access denied"

This prevents unauthorized access when group memberships are misconfigured.

## Examples

### Keycloak

```bash
export CRYSTAL_FORGE_OIDC_ROLES_CLAIM="groups"
export CRYSTAL_FORGE_ROLE_MAPPING='{"cf-admins":"admin","cf-operators":"operator","cf-users":"viewer"}'
export CRYSTAL_FORGE_DEFAULT_ROLE="viewer"
```

### Microsoft Entra ID (Azure AD)

```bash
export CRYSTAL_FORGE_OIDC_ROLES_CLAIM="roles"  # Azure uses "roles" claim
export CRYSTAL_FORGE_ROLE_MAPPING='{"CrystalForge.Admins":"admin","CrystalForge.Operators":"operator"}'
# No default role - deny users without explicit role assignment
```

### Authentik

```bash
export CRYSTAL_FORGE_OIDC_ROLES_CLAIM="groups"
export CRYSTAL_FORGE_ROLE_MAPPING='{"authentik Admins":"admin","Crystal Forge Operators":"operator"}'
export CRYSTAL_FORGE_DEFAULT_ROLE="viewer"
```

## Testing Role Mapping

1. Log in with OIDC
2. Check server logs for role assignment:
   ```
   Mapped OIDC groups ["cf-admins", "cf-users"] to role Admin for user <uuid>
   ```
3. Verify role in database:
   ```sql
   SELECT u.email, r.role 
   FROM users u 
   JOIN user_role_assignments r ON u.id = r.user_id;
   ```

## Troubleshooting

**Login fails with "No matching role for OIDC groups"**
- Check OIDC groups claim is being sent by provider
- Verify `CRYSTAL_FORGE_OIDC_ROLES_CLAIM` matches provider's claim name
- Check `CRYSTAL_FORGE_ROLE_MAPPING` includes user's groups
- Consider setting `CRYSTAL_FORGE_DEFAULT_ROLE` for fallback

**Role doesn't update after group change**
- Roles update on login, not in real-time
- User must log out and log back in
- Check server logs for role mapping decision

**Invalid JSON in CRYSTAL_FORGE_ROLE_MAPPING**
- Role mapping will be empty (safe-deny)
- All logins will fail unless `CRYSTAL_FORGE_DEFAULT_ROLE` is set
- Check JSON syntax with `jq`:
  ```bash
  echo "$CRYSTAL_FORGE_ROLE_MAPPING" | jq .
  ```
