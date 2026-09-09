"""PlaneClient wrapper with error handling and config integration."""

from __future__ import annotations

from plane.client import PlaneClient
from plane.errors import HttpError, PlaneError

from planecli.config import Config, load_config
from planecli.exceptions import APIError, AuthenticationError, PlaneCLIError

_client: PlaneClient | None = None
_config: Config | None = None


def get_config(
    *,
    base_url: str | None = None,
    api_key: str | None = None,
    workspace: str | None = None,
) -> Config:
    """Get or create the global config."""
    global _config
    if _config is None:
        _config = load_config(base_url=base_url, api_key=api_key, workspace=workspace)
    return _config


def get_client(
    *,
    base_url: str | None = None,
    api_key: str | None = None,
    workspace: str | None = None,
) -> PlaneClient:
    """Get or create the global PlaneClient singleton."""
    global _client
    if _client is None:
        config = get_config(base_url=base_url, api_key=api_key, workspace=workspace)
        _client = PlaneClient(base_url=config.base_url, api_key=config.api_key)
    return _client


def get_workspace() -> str:
    """Get the workspace slug from config."""
    config = get_config()
    return config.workspace


_SENSITIVE_BODY_KEYS = {"api_key", "authorization", "token", "access_token", "password"}


def _response_error_detail(response: object | None) -> str | None:
    """Extract human-readable error details from an SDK error response body.

    The plane-sdk attaches the parsed JSON body (or raw text) of a failed
    response to HttpError.response. Field-level errors look like
    {"description": ["This field is required."]}; DRF-style bodies carry a
    top-level "detail" string. Returns None when nothing useful is there.
    """
    if isinstance(response, dict):
        parts: list[str] = []
        detail = response.get("detail")
        if isinstance(detail, str) and detail.strip():
            parts.append(detail.strip())
        for key, value in response.items():
            if key == "detail" or key.lower() in _SENSITIVE_BODY_KEYS:
                continue
            if isinstance(value, list):
                messages = [str(item).strip() for item in value if str(item).strip()]
                if messages:
                    parts.append(f"{key}: {'; '.join(messages)}")
            elif isinstance(value, str) and value.strip():
                parts.append(f"{key}: {value.strip()}")
        return " | ".join(parts) if parts else None
    if isinstance(response, str) and response.strip():
        return response.strip()[:200]
    return None


def handle_api_error(err: PlaneError) -> PlaneCLIError:
    """Convert a Plane SDK error to a PlaneCLI error."""
    if isinstance(err, HttpError):
        if err.status_code == 401:
            return AuthenticationError()
        if err.status_code == 429:
            return APIError(
                "Rate limited by Plane API after multiple retries. Try again later.",
                status_code=429,
            )
        message = str(err)
        detail = _response_error_detail(err.response)
        if detail:
            message = f"{message} — {detail}"
        return APIError(message, status_code=err.status_code)
    return APIError(str(err))
