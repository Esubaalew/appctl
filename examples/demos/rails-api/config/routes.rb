Rails.application.routes.draw do
  namespace :api do
    namespace :v1 do
      resources :posts
      resources :comments, only: [:index, :show, :create]
    end
  end
end
