terraform {
  required_providers {
    null = {
      source  = "hashicorp/null"
      version = "~> 3.2"
    }
  }
}

variable "name" {
  type    = string
  default = "demo"
}

resource "null_resource" "example" {
  triggers = {
    name = var.name
  }
  unknown_attribute = true
}

output "id" {
  value = null_resource.example.id
}
