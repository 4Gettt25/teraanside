terraform {
  required_providers {
    null = {
      source  = "hashicorp/null"
      version = "~> 3.2"
    }
  }
}

variable "greeting" {
  type        = string
  description = "Text written into the trigger of the demo resource."
  default     = "hello"
}

resource "null_resource" "demo" {
  triggers = {
    greeting = var.greeting
  }
}

output "demo_id" {
  value = null_resource.demo.id
}
