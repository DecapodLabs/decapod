# GitHub App Setup for Auto-Tag Workflow

The auto-tag workflow requires a GitHub App to create pull requests that can trigger other workflows.

## Why GitHub App Instead of GITHUB_TOKEN?

GitHub's default `GITHUB_TOKEN` cannot create PRs that trigger workflows (security feature to prevent infinite loops). A GitHub App token can bypass this limitation.

## Setup Steps

### 1. Create a GitHub App

1. Go to your organization settings: https://github.com/organizations/DecapodLabs/settings/apps
2. Click "New GitHub App"
3. Fill in the details:
   - **Name**: `Decapod Release Manager` (or similar)
   - **Homepage URL**: `https://github.com/DecapodLabs/decapod`
   - **Webhook**: Uncheck "Active"

### 2. Set Permissions

Under "Repository permissions":
- **Contents**: Read & Write (for pushing branches)
- **Pull requests**: Read & Write (for creating PRs)
- **Metadata**: Read-only (automatically selected)

### 3. Install the App

1. After creating, click "Install App" in the left sidebar
2. Select "Only select repositories"
3. Choose `DecapodLabs/decapod`
4. Click "Install"

### 4. Generate Private Key

1. In the app settings, scroll to "Private keys"
2. Click "Generate a private key"
3. Save the downloaded `.pem` file securely

### 5. Add Secrets to Repository

1. Go to repository settings: https://github.com/DecapodLabs/decapod/settings/secrets/actions
2. Add two secrets:

   **GH_APP_ID**:
   - Value: The App ID (found at top of app settings page)

   **GH_APP_PRIVATE_KEY**:
   - Value: The entire contents of the `.pem` file
   - Include the `-----BEGIN RSA PRIVATE KEY-----` and `-----END RSA PRIVATE KEY-----` lines

### 6. Test the Workflow

Merge a PR to master and the auto-tag workflow should:
1. Generate a GitHub App token
2. Create a bump branch
3. Automatically create a PR for the version bump

## Troubleshooting

- **Error: "Bad credentials"**: Check that both secrets are set correctly
- **Error: "Resource not accessible by integration"**: Check app permissions (Contents & PRs need Write access)
- **PR doesn't trigger workflows**: This is expected with GITHUB_TOKEN, but should work with GitHub App token
