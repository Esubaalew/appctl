require_relative "boot"
require "rails/all"

Bundler.require(*Rails.groups)

module DemoApi
  class Application < Rails::Application
    config.load_defaults 7.1
    config.api_only = true

    config.middleware.insert_before 0, Rack::Cors do
      allow do
        origins "*"
        resource "*", headers: :any, methods: [:get, :post, :put, :patch, :delete, :options]
      end
    end
  end
end
