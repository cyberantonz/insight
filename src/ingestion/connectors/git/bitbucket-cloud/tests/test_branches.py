from airbyte_cdk.models import SyncMode
from conftest import branch
from source_bitbucket_cloud.streams.base import repository_bucket
from source_bitbucket_cloud.streams.commit_branch_reachability import CommitBranchReachabilityStream


def test_branch_snapshot_reads_provider_and_marks_default(branches_stream, client, repo):
    client.branch_values[repo.uuid] = [branch("main", "a"), branch("release", "b")]
    records = list(
        branches_stream.read_records(SyncMode.full_refresh, stream_slice={"bucket_id": repository_bucket(repo.uuid)})
    )
    items = records[:-1]
    assert [item["name"] for item in items] == ["main", "release"]
    assert items[0]["is_default"] is True
    assert items[1]["is_default"] is False
    assert records[-1]["snapshot_item_count"] == 2
    assert set(items[0]) <= set(branches_stream.get_json_schema()["properties"])


def test_branch_snapshot_counts_duplicate_entities_once(branches_stream, client, repo):
    client.branch_values[repo.uuid] = [branch("main", "a"), branch("main", "a")]
    records = list(
        branches_stream.read_records(SyncMode.full_refresh, stream_slice={"bucket_id": repository_bucket(repo.uuid)})
    )
    assert records[-1]["snapshot_item_count"] == 1


def test_reachability_emits_commits_for_every_changed_branch(stream_args, client, repo):
    stream = CommitBranchReachabilityStream(**stream_args)
    client.branch_values[repo.uuid] = [branch("main", "m1"), branch("release", "r1")]
    client.commit_values = [{"hash": "c1", "date": "2026-06-01"}]
    records = list(stream.read_records(SyncMode.incremental, stream_slice={"bucket_id": repository_bucket(repo.uuid)}))
    assert {record["branch_name"] for record in records} == {"main", "release"}
    assert all(record["reachability_action"] == "added" for record in records)
    assert stream.state["repositories"][repo.uuid]["heads"] == {"main": "m1", "release": "r1"}


def test_reachability_records_deleted_branch(stream_args, client, repo):
    stream = CommitBranchReachabilityStream(**stream_args)
    stream.state = {"version": 2, "bucket_count": 8, "repositories": {repo.uuid: {"heads": {"release": "old"}}}}
    client.branch_values[repo.uuid] = []
    records = list(stream.read_records(SyncMode.incremental, stream_slice={"bucket_id": repository_bucket(repo.uuid)}))
    assert records[0]["branch_name"] == "release"
    assert records[0]["reachability_action"] == "branch_deleted"
    assert records[0]["commit_sha"] is None
