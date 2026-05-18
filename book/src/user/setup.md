# Set up jj-spr

In the Jujutsu repo you want to use jj-spr in, run `jj spr init`; this will ask you several questions.

## GitHub Authentication

jj-spr needs to authenticate with GitHub to create and manage pull requests on your behalf. There are two authentication methods available:

### Option 1: GitHub CLI (Recommended)

If you have the [GitHub CLI (`gh`)](https://cli.github.com/) installed and authenticated, jj-spr can use it automatically:

1. Install and authenticate with GitHub CLI:
   ```shell
   gh auth login
   ```

2. Run `jj spr init` and select "Yes" when asked to use GitHub CLI authentication

This is the recommended approach because:
- No need to manually create tokens
- `gh` handles token management and renewal
- More secure - tokens are managed by GitHub's official tool

### Option 2: Personal Access Token (Manual)

If you prefer not to use the GitHub CLI, you can manually create a personal access token:

**Required token scopes:**
- `repo` - Full control of private repositories (required for creating and updating PRs)
- `workflow` - Update GitHub Actions workflow files (required only if your changes include files in `.github/workflows/`)

**Setup steps:**
1. [Create a new token](https://github.com/settings/tokens/new?scopes=repo,workflow&description=jj-spr) (this link pre-selects the correct scopes)
2. Copy the generated token
3. Run `jj spr init` and paste the token when prompted

**Security note:** The token will be stored in your repository's git config (`.git/config`). Make sure this file is not accidentally committed or shared.

For more details on creating tokens, see the [GitHub documentation](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/creating-a-personal-access-token).

## Running `jj spr init`

The rest of the settings that `jj spr init` asks for have sensible defaults, so almost all users can simply accept the defaults. The most common situation where you would need to diverge from the defaults is if the remote representing GitHub is not called `origin`.

See the [Configuration](../reference/configuration.md) reference page for full details about the available settings.

## Updating Configuration

After running `jj spr init`, your settings are stored and you're ready to go. If you need to change settings later:

**Easiest method - Rerun init:**
```shell
jj spr init
```
The defaults will be your current settings, so you can easily update what you need.

**Alternative - Use git config:**
```shell
# View current settings
git config --list | grep spr

# Update individual settings
git config spr.githubAuthToken "your-new-token"
```

For detailed configuration options, see the [Configuration Reference](../reference/configuration.md).
