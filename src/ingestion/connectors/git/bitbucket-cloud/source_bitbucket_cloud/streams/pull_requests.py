from __future__ import annotations

from collections.abc import Iterable, Mapping
from typing import Any

from airbyte_cdk.models import SyncMode

from source_bitbucket_cloud.streams.base import schema, truncate, unique_key
from source_bitbucket_cloud.streams.pr_base import PullRequestStateStream


class PullRequestsStream(PullRequestStateStream):
    name = "pull_requests"

    def read_records(
        self,
        sync_mode: SyncMode,
        cursor_field: list[str] | None = None,
        stream_slice: Mapping[str, Any] | None = None,
        stream_state: Mapping[str, Any] | None = None,
    ) -> Iterable[Mapping[str, Any]]:
        del sync_mode, cursor_field, stream_state
        bucket_id = int((stream_slice or {}).get("bucket_id", 0))
        repositories = self.repositories_for_slice(stream_slice)
        for repo in repositories:
            selected, new_state = self.selected_pull_requests(repo, self.repository_state(repo))
            for pr in selected:
                yield self._record(repo, pr)
            self.commit_repository_state(repo, new_state)
        self.prune_bucket_state(bucket_id, repositories)
        self.log_state_size()

    def _record(self, repo, pr: Mapping[str, Any]) -> Mapping[str, Any]:
        pr_id = pr.get("id")
        author = pr.get("author") or {}
        closed_by = pr.get("closed_by") or {}
        source = pr.get("source") or {}
        destination = pr.get("destination") or {}
        participants = []
        for participant in pr.get("participants") or []:
            user = participant.get("user") or {}
            participants.append(
                {
                    "display_name": user.get("display_name"),
                    "uuid": user.get("uuid"),
                    "account_id": user.get("account_id"),
                    "nickname": user.get("nickname"),
                    "role": participant.get("role"),
                    "approved": participant.get("approved"),
                    "state": participant.get("state"),
                    "participated_on": participant.get("participated_on"),
                }
            )
        entity_key = unique_key(self._tenant_id, self._source_id, repo.uuid, pr_id)
        return self.item(
            entity_key=entity_key,
            repository_uuid=repo.uuid,
            workspace_uuid=repo.workspace_uuid,
            id=pr_id,
            title=pr.get("title"),
            description=truncate(pr.get("description")),
            state=pr.get("state"),
            created_on=pr.get("created_on"),
            updated_on=pr.get("updated_on"),
            author_display_name=author.get("display_name"),
            author_uuid=author.get("uuid"),
            author_account_id=author.get("account_id"),
            closed_by_display_name=closed_by.get("display_name"),
            closed_by_uuid=closed_by.get("uuid"),
            closed_by_account_id=closed_by.get("account_id"),
            source_branch=(source.get("branch") or {}).get("name"),
            destination_branch=(destination.get("branch") or {}).get("name"),
            source_commit_hash=(source.get("commit") or {}).get("hash"),
            destination_commit_hash=(destination.get("commit") or {}).get("hash"),
            merge_commit_hash=(pr.get("merge_commit") or {}).get("hash"),
            task_count=pr.get("task_count"),
            draft=pr.get("draft"),
            queued=pr.get("queued"),
            close_source_branch=pr.get("close_source_branch"),
            reason=pr.get("reason"),
            reviewers=pr.get("reviewers") or [],
            comment_count=pr.get("comment_count"),
            participants=participants,
            workspace=repo.workspace,
            repo_slug=repo.slug,
        )

    def get_json_schema(self) -> Mapping[str, Any]:
        nullable_string = {"type": ["null", "string"]}
        return schema(
            {
                "id": {"type": ["null", "integer"]},
                "title": nullable_string,
                "description": nullable_string,
                "state": nullable_string,
                "created_on": nullable_string,
                "updated_on": nullable_string,
                "author_display_name": nullable_string,
                "author_uuid": nullable_string,
                "author_account_id": nullable_string,
                "closed_by_display_name": nullable_string,
                "closed_by_uuid": nullable_string,
                "closed_by_account_id": nullable_string,
                "source_branch": nullable_string,
                "destination_branch": nullable_string,
                "source_commit_hash": nullable_string,
                "destination_commit_hash": nullable_string,
                "merge_commit_hash": nullable_string,
                "task_count": {"type": ["null", "integer"]},
                "draft": {"type": ["null", "boolean"]},
                "queued": {"type": ["null", "boolean"]},
                "close_source_branch": {"type": ["null", "boolean"]},
                "reason": nullable_string,
                "reviewers": {"type": ["null", "array"]},
                "comment_count": {"type": ["null", "integer"]},
                "participants": {"type": ["null", "array"]},
                "workspace": nullable_string,
                "repo_slug": nullable_string,
            }
        )
