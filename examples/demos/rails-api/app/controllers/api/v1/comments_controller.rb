module Api
  module V1
    class CommentsController < ApplicationController
      def index
        render json: Comment.all
      end

      def show
        render json: Comment.find(params[:id])
      end

      def create
        comment = Comment.create!(comment_params)
        render json: comment, status: :created
      end

      private

      def comment_params
        params.require(:comment).permit(:body, :post_id)
      end
    end
  end
end
