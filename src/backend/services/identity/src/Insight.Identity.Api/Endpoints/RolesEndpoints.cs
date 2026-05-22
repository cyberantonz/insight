using FluentValidation;
using Insight.Identity.Api.Auth;
using Insight.Identity.Api.Contracts;
using Insight.Identity.Domain.Services;
using Insight.Identity.Infrastructure.MariaDb;
using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Http;
using Microsoft.AspNetCore.Routing;
using Microsoft.Extensions.Logging;

namespace Insight.Identity.Api.Endpoints;

/// <summary>
/// CRUD over the <c>roles</c> catalogue. Admin-only; hard-DELETE with
/// a 422 <c>urn:insight:error:role_in_use</c> guard against orphaning
/// active <c>person_roles</c> assignments. See ADR-0013.
/// </summary>
public static class RolesEndpoints
{
    private const string RoleNameExistsUrn = "urn:insight:error:role_name_exists";
    private const string RoleInUseUrn      = "urn:insight:error:role_in_use";

    public static IEndpointRouteBuilder MapRoleEndpoints(this IEndpointRouteBuilder app)
    {
        ArgumentNullException.ThrowIfNull(app);

        app.MapPost("/v1/roles", async (
            CreateRoleCommandModel body,
            HttpContext http,
            CallerAdminCheck admin,
            IValidator<CreateRoleCommandModel> validator,
            RolesRepository repo,
            ILoggerFactory loggerFactory,
            CancellationToken ct) =>
        {
            var gate = await admin.CheckAsync(http, ct).ConfigureAwait(false);
            if (gate is not AdminCheckResult.IsAdmin) return EndpointHelpers.GateResult(gate);

            var validation = await validator.ValidateAsync(body, ct).ConfigureAwait(false);
            if (!validation.IsValid) return EndpointHelpers.ValidationFailure(validation);

            // Pre-check for duplicate name so the response is 409 with
            // a friendly URN; the UNIQUE(name) index would also reject
            // but surface as an opaque 500 MySqlException.
            var existing = await repo.GetByNameAsync(body.Name, ct).ConfigureAwait(false);
            if (existing is not null)
            {
                return Results.Json(new ProblemResponse(
                    Type: RoleNameExistsUrn,
                    Title: "Conflict",
                    Status: StatusCodes.Status409Conflict,
                    Detail: $"role name '{body.Name}' already exists"),
                    statusCode: StatusCodes.Status409Conflict);
            }

            var id = await repo.InsertRoleAsync(body.Name, ct).ConfigureAwait(false);
            EndpointHelpers.Audit(loggerFactory, "roles.create",
                ("role_id", id),
                ("name", body.Name),
                ("author_person_id", EndpointHelpers.ResolveCaller(http)!.Value));
            return Results.Created($"/v1/roles/{id:D}", new RoleResponse(id, body.Name));
        });

        app.MapGet("/v1/roles", async (
            HttpContext http,
            CallerAdminCheck admin,
            RolesRepository repo,
            CancellationToken ct) =>
        {
            var gate = await admin.CheckAsync(http, ct).ConfigureAwait(false);
            if (gate is not AdminCheckResult.IsAdmin) return EndpointHelpers.GateResult(gate);

            var roles = await repo.ListAllAsync(ct).ConfigureAwait(false);
            var items = roles.Select(RoleResponse.From).ToList();
            return Results.Ok(new ListResponse<RoleResponse>(items, NextCursor: null));
        });

        app.MapDelete("/v1/roles/{id}", async (
            Guid id,
            HttpContext http,
            CallerAdminCheck admin,
            RolesRepository repo,
            ILoggerFactory loggerFactory,
            CancellationToken ct) =>
        {
            var gate = await admin.CheckAsync(http, ct).ConfigureAwait(false);
            if (gate is not AdminCheckResult.IsAdmin) return EndpointHelpers.GateResult(gate);

            var existing = await repo.GetRoleByIdAsync(id, ct).ConfigureAwait(false);
            if (existing is null) return EndpointHelpers.NotFound("role", id);

            // `roles` is strict-minimum (no valid_to column) so DELETE
            // is hard. Refuse if any active assignment references this
            // role in any tenant — otherwise person_roles rows would
            // be orphaned (no FK declared, see DESIGN §3.8).
            var live = await repo.CountActiveAssignmentsByRoleAnyTenantAsync(id, ct).ConfigureAwait(false);
            if (live > 0)
            {
                return Results.Json(new ProblemResponse(
                    Type: RoleInUseUrn,
                    Title: "Unprocessable Entity",
                    Status: StatusCodes.Status422UnprocessableEntity,
                    Detail: $"role has {live} active assignment(s); revoke them before deletion"),
                    statusCode: StatusCodes.Status422UnprocessableEntity);
            }

            await repo.DeleteRoleAsync(id, ct).ConfigureAwait(false);
            EndpointHelpers.Audit(loggerFactory, "roles.delete",
                ("role_id", id),
                ("name", existing.Name),
                ("author_person_id", EndpointHelpers.ResolveCaller(http)!.Value));
            return Results.NoContent();
        });

        return app;
    }
}
