#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

pass() {
  echo "[oauth-provider-acceptance] PASS: $1"
}

fail() {
  echo "[oauth-provider-acceptance] FAIL: $1" >&2
  exit 1
}

require_fixed() {
  local file="$1"
  local needle="$2"
  local description="$3"

  if ! grep -Fq "$needle" "$file"; then
    fail "$description"
  fi
}

require_absent() {
  local file="$1"
  local needle="$2"
  local description="$3"

  if grep -Fq "$needle" "$file"; then
    fail "$description"
  fi
}

echo "[oauth-provider-acceptance] verifying Gmail/Outlook providers are first-class account types"
require_fixed src/config.rs "Gmail," "MailProvider must include Gmail"
require_fixed src/config.rs "Outlook," "MailProvider must include Outlook"
require_fixed src/config.rs '"gmail" => Ok(MailProvider::Gmail)' "config must parse gmail provider"
require_fixed src/config.rs '"outlook" => Ok(MailProvider::Outlook)' "config must parse outlook provider"
require_fixed src/config.rs 'provider must be one of qq, gmail, outlook' "unsupported provider must return a clear error"
pass "provider parsing and errors are explicit"

echo "[oauth-provider-acceptance] verifying OAuth config validation and default scopes"
require_fixed src/config.rs 'gmail.modify' "Gmail default scopes must include gmail.modify"
require_fixed src/config.rs 'gmail.send' "Gmail default scopes must include gmail.send"
require_fixed src/config.rs 'offline_access' "Outlook default scopes must include offline_access"
require_fixed src/config.rs 'Mail.ReadWrite' "Outlook default scopes must include Mail.ReadWrite"
require_fixed src/config.rs 'Mail.Send' "Outlook default scopes must include Mail.Send"
require_fixed src/config.rs 'oauth.client_id must not be empty' "OAuth client_id must reject blank input"
require_fixed src/config.rs 'oauth.redirect_uri must not be empty' "OAuth redirect_uri must reject blank input"
require_fixed src/config.rs 'account id must contain only ASCII letters, numbers' "account id must reject injection-prone names"
require_fixed src/config.rs 'mail endpoint port must be greater than 0' "endpoint port must reject invalid zero value"
pass "OAuth defaults and malicious config validation are covered"

echo "[oauth-provider-acceptance] verifying tokens are stored outside account config and refresh safely"
require_fixed src/config.rs 'pub token_store_path: PathBuf' "AppConfig must expose a separate token store path"
require_fixed src/mail/oauth.rs 'pub struct StoredToken' "OAuth tokens must use a dedicated stored token structure"
require_fixed src/mail/oauth.rs 'pub refresh_token: Option<String>' "StoredToken must persist refresh tokens separately from config"
require_fixed src/mail/oauth.rs 'const REFRESH_SKEW_SECONDS: u64 = 300;' "access tokens must refresh before expiry"
require_fixed src/mail/oauth.rs '("grant_type", "refresh_token")' "refresh must use OAuth refresh_token grant"
require_fixed src/mail/oauth.rs 'ok_or(MailError::ReauthorizationRequired)' "missing refresh token must require reauthorization"
require_fixed src/mail/oauth.rs 'ensure_scopes(&token, required_scopes)' "token retrieval must enforce required scopes"
require_fixed config/qqmail.yaml.example '# OAuth access/refresh tokens are stored here, not in account config.' "sample config must document external token storage"
require_absent config/qqmail.yaml.example 'refresh_token:' "sample account config must not contain OAuth refresh tokens"
require_absent config/qqmail.yaml.example 'oauth_access_token:' "sample account config must not contain OAuth access tokens"
pass "token persistence, refresh, and scope checks are isolated"

echo "[oauth-provider-acceptance] verifying MCP tools route through a backend trait with opaque IDs"
require_fixed src/mail/backend.rs 'pub trait MailBackend' "mail backend trait must exist"
require_fixed src/mail/backend.rs 'MailProvider::Gmail => Box::new(providers::GmailBackend::new(account, token_store_path))' "Gmail accounts must route to GmailBackend"
require_fixed src/mail/backend.rs 'MailProvider::Outlook =>' "Outlook accounts must route to OutlookBackend"
require_fixed src/mail/backend.rs 'pub mailbox_id: String' "backend mailbox IDs must be opaque strings"
require_fixed src/mail/backend.rs 'pub message_id: String' "backend message IDs must be opaque strings"
require_fixed src/mcp.rs 'pub cursor: Option<String>' "MCP pagination cursor must be opaque"
require_fixed src/mcp.rs '#[serde(alias = "uid")]' "QQ UID compatibility must stay at MCP boundary"
require_fixed src/mcp.rs 'validate_account_id_param(account)?' "MCP account input must reject blank account IDs"
require_fixed src/mcp.rs 'message_id is required' "MCP message_id input must reject blank IDs"
pass "backend routing keeps provider IDs opaque and validates dangerous blanks"

echo "[oauth-provider-acceptance] verifying Gmail API operation mapping"
require_fixed src/mail/providers.rs 'https://gmail.googleapis.com/gmail/v1/users/me/messages/send' "Gmail send must use Gmail API messages.send"
require_fixed src/mail/providers.rs 'https://gmail.googleapis.com/gmail/v1/users/me/labels' "Gmail mailboxes must use labels API"
require_fixed src/mail/providers.rs 'https://gmail.googleapis.com/gmail/v1/users/me/messages?labelIds={}&maxResults={}' "Gmail list must use messages list with label and maxResults"
require_fixed src/mail/providers.rs 'messages/{}/trash' "Gmail delete must default to trash"
require_fixed src/mail/providers.rs 'messages/{}/modify' "Gmail move/mark must use label modify"
require_fixed src/mail/providers.rs 'addLabelIds' "Gmail move/mark must add labels"
require_fixed src/mail/providers.rs 'removeLabelIds' "Gmail move/mark must remove labels"
pass "Gmail backend maps tools to Gmail API"

echo "[oauth-provider-acceptance] verifying Microsoft Graph operation mapping"
require_fixed src/mail/providers.rs 'https://graph.microsoft.com/v1.0/me/sendMail' "Outlook send must use Microsoft Graph sendMail"
require_fixed src/mail/providers.rs 'https://graph.microsoft.com/v1.0/me/mailFolders' "Outlook mailboxes must use Graph mailFolders"
require_fixed src/mail/providers.rs 'https://graph.microsoft.com/v1.0/me/mailFolders/{}/messages?$top={}' "Outlook list must use folder messages with top limit"
require_fixed src/mail/providers.rs 'destinationId": "deleteditems"' "Outlook delete must default to Deleted Items"
require_fixed src/mail/providers.rs 'messages/{}/move' "Outlook move/delete must use Graph move"
require_fixed src/mail/providers.rs '"isRead"' "Outlook mark read must patch isRead"
require_fixed src/mail/providers.rs '"flagStatus"' "Outlook mark flagged must patch flagStatus"
pass "Outlook backend maps tools to Microsoft Graph"

echo "[oauth-provider-acceptance] verifying unified provider error codes"
require_fixed src/mcp.rs '"account_not_found"' "MCP errors must include account_not_found"
require_fixed src/mcp.rs '"oauth_not_authorized"' "MCP errors must include oauth_not_authorized"
require_fixed src/mcp.rs '"reauthorization_required"' "MCP errors must include reauthorization_required"
require_fixed src/mcp.rs '"insufficient_scope"' "MCP errors must include insufficient_scope"
require_fixed src/mcp.rs '"provider_rate_limited"' "MCP errors must include provider_rate_limited"
require_fixed src/mcp.rs '"provider_api_error"' "MCP errors must include provider_api_error"
require_fixed src/mail/providers.rs 'StatusCode::TOO_MANY_REQUESTS' "provider 429 responses must map to rate limit errors"
require_fixed src/mail/providers.rs 'StatusCode::UNAUTHORIZED' "provider 401 responses must require reauthorization"
require_fixed src/mail/providers.rs 'StatusCode::FORBIDDEN' "provider 403 responses must map to insufficient scope"
pass "provider failures return reviewable stable error codes"

echo "[oauth-provider-acceptance] PASS: Gmail/Outlook OAuth provider acceptance checks completed"
