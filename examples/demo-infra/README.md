# demo-infra

Small mixed repository for trying IaC Studio (and for the M1 acceptance check).
It creates no real infrastructure: Terraform/OpenTofu only uses the `null` provider,
Ansible only targets `localhost` with `connection: local`.

- `terraform/` – root module with a variable, a `null_resource` and an output
- `site.yml` – playbook using the `hello` role
- `inventory/hosts.yml`, `group_vars/all.yml`

Open the folder in IaC Studio, trust it, let the setup wizard install the tools,
then open `terraform/main.tf` or `roles/hello/tasks/main.yml`: completion, hover
and diagnostics come from terraform-ls and ansible-language-server.
