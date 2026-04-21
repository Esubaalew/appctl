module Api
  module V1
    class PostsController < ApplicationController
      before_action :set_post, only: [:show, :update, :destroy]

      def index
        render json: Post.all
      end

      def show
        render json: @post
      end

      def create
        post = Post.create!(post_params)
        render json: post, status: :created
      end

      def update
        @post.update!(post_params)
        render json: @post
      end

      def destroy
        @post.destroy
        head :no_content
      end

      private

      def set_post
        @post = Post.find(params[:id])
      end

      def post_params
        params.require(:post).permit(:title, :body, :author_id, :published_at, :featured)
      end
    end
  end
end
