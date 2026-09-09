"""Tests for work item comment commands."""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from plane.errors import PlaneError

from planecli.commands.comments import _body_to_html, _enrich_comment
from planecli.exceptions import ValidationError
from planecli.utils.body_html import md_to_html


def test_body_to_html_wraps_single_line_in_one_paragraph():
    assert _body_to_html("hello") == "<p>hello</p>"


def test_body_to_html_splits_blank_lines_into_paragraphs():
    body = "first\n\nsecond\n\n\nthird"
    assert _body_to_html(body) == "<p>first</p><p>second</p><p>third</p>"


def test_body_to_html_converts_single_newline_to_br():
    body = "追加修复：\n1) first\n2) second"
    assert _body_to_html(body) == "<p>追加修复：<br/>1) first<br/>2) second</p>"


def test_body_to_html_linkifies_bare_url():
    body = "PR https://github.com/owner/repo/pull/12"
    assert (
        _body_to_html(body) == '<p>PR <a href="https://github.com/owner/repo/pull/12">'
        "https://github.com/owner/repo/pull/12</a></p>"
    )


def test_body_to_html_keeps_trailing_punctuation_out_of_link():
    body = "见 https://example.com/a。下一条"
    assert (
        _body_to_html(body)
        == '<p>见 <a href="https://example.com/a">https://example.com/a</a>。下一条</p>'
    )


def test_body_to_html_does_not_double_link_existing_anchor():
    body = '<a href="https://example.com">https://example.com</a>'
    assert _body_to_html(body) == f"<p>{body}</p>"


def test_body_to_html_linkifies_multiple_urls_in_one_line():
    body = "Issue https://a/1\nPR https://b/2"
    assert (
        _body_to_html(body) == '<p>Issue <a href="https://a/1">https://a/1</a><br/>'
        'PR <a href="https://b/2">https://b/2</a></p>'
    )


def test_body_to_html_converts_inline_backticks_to_code():
    body = "分支 `feat/x`，已推送"
    assert _body_to_html(body) == "<p>分支 <code>feat/x</code>，已推送</p>"


def test_body_to_html_escapes_html_inside_inline_code():
    body = "存 `<p>` 标签"
    assert _body_to_html(body) == "<p>存 <code>&lt;p&gt;</code> 标签</p>"


def test_body_to_html_does_not_linkify_url_inside_code():
    body = "`https://a/1` 是内部地址"
    assert _body_to_html(body) == "<p><code>https://a/1</code> 是内部地址</p>"


def test_body_to_html_converts_fenced_block_to_pre():
    body = "前\n\n```\ncode <b>x</b>\n```\n\n后"
    assert (
        _body_to_html(body) == "<p>前</p><pre><code>code &lt;b&gt;x&lt;/b&gt;</code></pre><p>后</p>"
    )


def test_body_to_html_fenced_block_ignores_language_hint():
    body = "```python\nprint(1)\n```"
    assert _body_to_html(body) == "<pre><code>print(1)</code></pre>"


def test_md_to_html_escapes_user_text_in_heading():
    assert (
        md_to_html("# <script>alert(1)</script>")
        == "<h1>&lt;script&gt;alert(1)&lt;/script&gt;</h1>"
    )


def test_md_to_html_escapes_user_text_in_paragraph():
    assert md_to_html('a <b href="x">y</b>') == "<p>a &lt;b href=&quot;x&quot;&gt;y&lt;/b&gt;</p>"


def test_md_to_html_headings_all_levels():
    assert md_to_html("# one") == "<h1>one</h1>"
    assert md_to_html("###### six") == "<h6>six</h6>"


def test_md_to_html_heading_strips_closing_hashes():
    assert md_to_html("## Title ##") == "<h2>Title</h2>"


def test_md_to_html_plain_hash_line_is_a_paragraph():
    assert md_to_html("#nospace") == "<p>#nospace</p>"


def test_md_to_html_blank_lines_split_paragraphs():
    assert md_to_html("one\n\n\ntwo") == "<p>one</p><p>two</p>"


def test_md_to_html_unordered_list_bullets():
    assert md_to_html("- a\n- b") == "<ul><li>a</li><li>b</li></ul>"
    assert md_to_html("* a\n+ b") == "<ul><li>a</li><li>b</li></ul>"


def test_md_to_html_ordered_list():
    assert md_to_html("1. first\n2. second") == "<ol><li>first</li><li>second</li></ol>"


def test_md_to_html_bullet_change_starts_new_list():
    assert md_to_html("- a\n\n1. b") == "<ul><li>a</li></ul><ol><li>b</li></ol>"


def test_md_to_html_list_item_content_gets_inline_markup():
    assert md_to_html("- `x` and https://a/1") == (
        '<ul><li><code>x</code> and <a href="https://a/1">https://a/1</a></li></ul>'
    )


def test_md_to_html_fenced_block_becomes_pre_code():
    html = md_to_html("before\n\n```py\nprint('<x>')\n```\n\nafter")
    assert html == "<p>before</p><pre><code>print(&#x27;&lt;x&gt;&#x27;)</code></pre><p>after</p>"


def test_md_to_html_inline_code_is_escaped_and_not_linkified():
    html = md_to_html("use `https://x.io <b>` here")
    assert html == "<p>use <code>https://x.io &lt;b&gt;</code> here</p>"


def test_md_to_html_linkifies_bare_url():
    html = md_to_html("see https://example.com/a, ok")
    assert html == '<p>see <a href="https://example.com/a">https://example.com/a</a>, ok</p>'


def test_md_to_html_bold_and_italic():
    assert md_to_html("**bold** and *italic*") == "<p><strong>bold</strong> and <em>italic</em></p>"
    assert md_to_html("__bold__ and _italic_") == "<p><strong>bold</strong> and <em>italic</em></p>"


def test_md_to_html_underscore_inside_word_is_not_italic():
    assert md_to_html("snake_case_name") == "<p>snake_case_name</p>"


def test_enrich_comment_resolves_actor_name_from_members_map():
    members_map = {"user-1": "Alice"}
    result = _enrich_comment({"actor": "user-1", "comment_html": "<p>hello</p>"}, members_map)
    assert result["actor_name"] == "Alice"
    assert result["body_text"] == "hello"


def test_enrich_comment_falls_back_to_uuid_without_map():
    result = _enrich_comment({"actor": "user-1", "comment_html": "<p>hi</p>"})
    assert result["actor_name"] == "user-1"


def test_enrich_comment_falls_back_to_uuid_when_member_missing():
    result = _enrich_comment({"actor": "user-x", "comment_html": ""}, {"user-1": "Alice"})
    assert result["actor_name"] == "user-x"
    assert result["body_text"] == ""


@patch("planecli.cache.cached_list_members", new_callable=AsyncMock)
@patch("planecli.cache.cached_list_comments", new_callable=AsyncMock)
async def test_fetch_issue_comments_sorts_and_resolves(mock_comments, mock_members):
    from planecli.commands.comments import fetch_issue_comments

    mock_members.return_value = [
        {"id": "u1", "display_name": "Alice"},
        {"id": "u2", "display_name": "Bob"},
    ]
    # Returned out of order; helper must sort oldest -> newest
    mock_comments.return_value = [
        {
            "id": "c2",
            "actor": "u2",
            "comment_html": "<p>later</p>",
            "created_at": "2026-02-11T10:00:00Z",
        },
        {
            "id": "c1",
            "actor": "u1",
            "comment_html": "<p>earlier</p>",
            "created_at": "2026-02-10T10:00:00Z",
        },
    ]

    result = await fetch_issue_comments("ws", "p1", "item-1")

    assert [c["id"] for c in result] == ["c1", "c2"]  # chronological
    assert result[0]["actor_name"] == "Alice"
    assert result[0]["body_text"] == "earlier"
    assert result[1]["actor_name"] == "Bob"


@patch("planecli.cache.cached_list_members", new_callable=AsyncMock)
@patch("planecli.cache.cached_list_comments", new_callable=AsyncMock)
async def test_fetch_issue_comments_returns_all(mock_comments, mock_members):
    from planecli.commands.comments import fetch_issue_comments

    mock_members.return_value = []
    mock_comments.return_value = [
        {
            "id": f"c{i}",
            "actor": "u1",
            "comment_html": "<p>x</p>",
            "created_at": f"2026-02-{i:02d}T00:00:00Z",
        }
        for i in range(1, 31)
    ]

    result = await fetch_issue_comments("ws", "p1", "item-1")
    assert len(result) == 30  # no truncation


@patch("planecli.cache.cached_list_members", new_callable=AsyncMock)
@patch("planecli.cache.cached_list_comments", new_callable=AsyncMock)
async def test_fetch_issue_comments_raises_on_failure(mock_comments, mock_members):
    from planecli.commands.comments import fetch_issue_comments

    mock_members.return_value = []
    mock_comments.side_effect = PlaneError("boom")

    with pytest.raises(PlaneError):
        await fetch_issue_comments("ws", "p1", "item-1")


@patch("planecli.cache.cached_list_members", new_callable=AsyncMock)
@patch("planecli.cache.cached_list_comments", new_callable=AsyncMock)
async def test_fetch_issue_comments_degrades_when_members_fail(mock_comments, mock_members):
    """A members-list failure is a secondary-enrichment concern: it must not
    take down comments that already loaded successfully (names fall back to
    the raw actor UUID, same as the no-map case)."""
    from planecli.commands.comments import fetch_issue_comments

    mock_members.side_effect = PlaneError("members unavailable")
    mock_comments.return_value = [
        {
            "id": "c1",
            "actor": "u1",
            "comment_html": "<p>hi</p>",
            "created_at": "2026-02-10T10:00:00Z",
        },
    ]

    result = await fetch_issue_comments("ws", "p1", "item-1")

    assert len(result) == 1
    assert result[0]["actor_name"] == "u1"  # fell back to raw UUID


@patch("planecli.commands.comments.output")
@patch("planecli.commands.comments.fetch_issue_comments", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_ls_tail_slices_most_recent(
    mock_client, mock_ws, mock_resolve, mock_fetch, mock_output
):
    from planecli.commands.comments import list_

    mock_resolve.return_value = ({"id": "item-1"}, "p1")
    mock_fetch.return_value = [
        {"id": "c1", "created_at": "t1"},
        {"id": "c2", "created_at": "t2"},
        {"id": "c3", "created_at": "t3"},
    ]

    await list_("ABC-1", limit=2)

    mock_fetch.assert_awaited_once_with("ws", "p1", "item-1")
    data = mock_output.call_args[0][0]
    assert [c["id"] for c in data] == ["c2", "c3"]  # most recent 2, still chronological


@pytest.mark.parametrize("limit", [0, -2])
@patch("planecli.commands.comments.output")
@patch("planecli.commands.comments.fetch_issue_comments", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_ls_non_positive_limit_returns_none(
    mock_client, mock_ws, mock_resolve, mock_fetch, mock_output, limit
):
    """--limit 0 or negative means "no results", matching `wi ls`'s
    `data[:limit]` semantics (where limit=0 also yields an empty list) —
    not "all comments" (0 is falsy) and not a nonsensical reversed slice."""
    from planecli.commands.comments import list_

    mock_resolve.return_value = ({"id": "item-1"}, "p1")
    mock_fetch.return_value = [
        {"id": "c1", "created_at": "t1"},
        {"id": "c2", "created_at": "t2"},
        {"id": "c3", "created_at": "t3"},
    ]

    await list_("ABC-1", limit=limit)

    data = mock_output.call_args[0][0]
    assert data == []


@patch("planecli.commands.comments.output")
@patch("planecli.commands.comments.fetch_issue_comments", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_ls_hard_fails_on_plane_error(
    mock_client, mock_ws, mock_resolve, mock_fetch, mock_output
):
    from planecli.commands.comments import list_
    from planecli.exceptions import PlaneCLIError

    mock_resolve.return_value = ({"id": "item-1"}, "p1")
    mock_fetch.side_effect = PlaneError("boom")

    with pytest.raises(PlaneCLIError):
        await list_("ABC-1", limit=50)
    mock_output.assert_not_called()


@patch("planecli.cache.invalidate_resource", new_callable=AsyncMock)
@patch("planecli.commands.comments.run_sdk", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_create_invalidates_cache(
    mock_client, mock_ws, mock_resolve, mock_run_sdk, mock_invalidate
):
    from planecli.commands.comments import create

    mock_resolve.return_value = ({"id": "item-1"}, "p1")
    mock_run_sdk.return_value = MagicMock(
        model_dump=lambda: {"id": "c1", "comment_html": "<p>x</p>", "actor": "u1"}
    )

    await create("ABC-1", body="hello")

    mock_invalidate.assert_awaited_once_with("comments", "ws", "p1", "item-1")


@patch("planecli.cache.invalidate_resource", new_callable=AsyncMock)
@patch("planecli.commands.comments.run_sdk", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_update_invalidates_cache(
    mock_client, mock_ws, mock_resolve, mock_run_sdk, mock_invalidate
):
    from planecli.commands.comments import update

    mock_resolve.return_value = ({"id": "item-1"}, "p1")
    mock_run_sdk.return_value = MagicMock(
        model_dump=lambda: {"id": "c1", "comment_html": "<p>x</p>", "actor": "u1"}
    )

    await update("comment-1", issue="ABC-1", body="edited")

    mock_invalidate.assert_awaited_once_with("comments", "ws", "p1", "item-1")


@patch("planecli.cache.invalidate_resource", new_callable=AsyncMock)
@patch("planecli.commands.comments.run_sdk", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_create_with_body_md_converts_markdown(
    mock_client, mock_ws, mock_resolve, mock_run_sdk, mock_invalidate
):
    from planecli.commands.comments import create

    mock_resolve.return_value = ({"id": "item-1"}, "p1")
    mock_run_sdk.return_value = MagicMock(
        model_dump=lambda: {"id": "c1", "comment_html": "<h1>x</h1>", "actor": "u1"}
    )

    await create("ABC-1", body_md="# Title\n\n- a\n- b")

    comment_data = mock_run_sdk.call_args[0][4]
    assert comment_data.comment_html == "<h1>Title</h1><ul><li>a</li><li>b</li></ul>"


@patch("planecli.cache.invalidate_resource", new_callable=AsyncMock)
@patch("planecli.commands.comments.run_sdk", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_update_with_body_md_converts_markdown(
    mock_client, mock_ws, mock_resolve, mock_run_sdk, mock_invalidate
):
    from planecli.commands.comments import update

    mock_resolve.return_value = ({"id": "item-1"}, "p1")
    mock_run_sdk.return_value = MagicMock(
        model_dump=lambda: {"id": "c1", "comment_html": "<h2>x</h2>", "actor": "u1"}
    )

    await update("comment-1", issue="ABC-1", body_md="## Notes")

    update_data = mock_run_sdk.call_args[0][5]
    assert update_data.comment_html == "<h2>Notes</h2>"


async def test_comment_create_rejects_body_and_body_md_together():
    from planecli.commands.comments import create

    with pytest.raises(ValidationError, match="mutually exclusive"):
        await create("ABC-1", body="plain", body_md="# md")


async def test_comment_create_requires_a_body_flag():
    from planecli.commands.comments import create

    with pytest.raises(ValidationError, match="required"):
        await create("ABC-1")


async def test_comment_update_rejects_body_and_body_md_together():
    from planecli.commands.comments import update

    with pytest.raises(ValidationError, match="mutually exclusive"):
        await update("comment-1", issue="ABC-1", body="plain", body_md="# md")


@patch("planecli.cache.invalidate_resource", new_callable=AsyncMock)
@patch("planecli.commands.comments.run_sdk", new_callable=AsyncMock)
@patch(
    "planecli.commands.comments.resolve_work_item_across_projects_async",
    new_callable=AsyncMock,
)
@patch("planecli.commands.comments.get_workspace", return_value="ws")
@patch("planecli.commands.comments.get_client")
async def test_comment_delete_invalidates_cache(
    mock_client, mock_ws, mock_resolve, mock_run_sdk, mock_invalidate
):
    from planecli.commands.comments import delete

    mock_resolve.return_value = ({"id": "item-1"}, "p1")

    await delete("comment-1", issue="ABC-1")

    mock_invalidate.assert_awaited_once_with("comments", "ws", "p1", "item-1")
