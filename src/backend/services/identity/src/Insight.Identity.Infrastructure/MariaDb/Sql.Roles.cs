namespace Insight.Identity.Infrastructure.MariaDb;

/// <summary>
/// SQL for the `roles` and `person_roles` tables (#346 step 1).
/// `roles` is global (no tenant column); `person_roles` is per-tenant.
/// </summary>
internal static class SqlRoles
{
    public const string RoleByName = """
        SELECT role_id, name
        FROM roles
        WHERE name = @name
        LIMIT 1
        """;

    public const string ListAllRoles = """
        SELECT role_id, name
        FROM roles
        ORDER BY name
        """;

    public const string HasActivePersonRole = """
        SELECT EXISTS (
            SELECT 1
            FROM person_roles
            WHERE insight_tenant_id = @tenant_id
              AND person_id         = @person_id
              AND role_id           = @role_id
              AND valid_to IS NULL
        )
        """;

    public const string ActivePersonRolesByPerson = """
        SELECT person_role_id, insight_tenant_id, person_id, role_id,
               valid_from, valid_to, author_person_id, reason, created_at
        FROM person_roles
        WHERE insight_tenant_id = @tenant_id
          AND person_id         = @person_id
          AND valid_to IS NULL
        """;

    public const string RoleById = """
        SELECT role_id, name
        FROM roles
        WHERE role_id = @role_id
        LIMIT 1
        """;

    public const string InsertRole = """
        INSERT INTO roles (role_id, name)
        VALUES (@role_id, @name)
        """;

    public const string DeleteRole = """
        DELETE FROM roles
        WHERE role_id = @role_id
        """;

    public const string CountActivePersonRolesByRole = """
        SELECT COUNT(*)
        FROM person_roles
        WHERE insight_tenant_id = @tenant_id
          AND role_id           = @role_id
          AND valid_to IS NULL
        """;

    public const string CountActivePersonRolesByRoleAnyTenant = """
        SELECT COUNT(*)
        FROM person_roles
        WHERE role_id    = @role_id
          AND valid_to IS NULL
        """;

    private const string PersonRoleColumnList =
        "person_role_id, insight_tenant_id, person_id, role_id, " +
        "valid_from, valid_to, author_person_id, reason, created_at";

    public const string PersonRoleById = $"""
        SELECT {PersonRoleColumnList}
        FROM person_roles
        WHERE person_role_id = @person_role_id
        LIMIT 1
        """;

    public const string PersonRoleListBase = $"""
        SELECT {PersonRoleColumnList}
        FROM person_roles
        WHERE insight_tenant_id = @tenant_id
        """;

    public const string InsertPersonRole = """
        INSERT INTO person_roles
            (person_role_id, insight_tenant_id, person_id, role_id,
             valid_from, valid_to, author_person_id, reason)
        VALUES
            (@person_role_id, @tenant_id, @person_id, @role_id,
             IFNULL(@valid_from, UTC_TIMESTAMP(6)), NULL, @author_person_id, @reason)
        """;

    public const string SoftDeletePersonRole = """
        UPDATE person_roles
        SET valid_to = UTC_TIMESTAMP(6),
            reason   = COALESCE(@reason, reason)
        WHERE person_role_id = @person_role_id
          AND valid_to IS NULL
        """;
}
