<?php

namespace App\Http\Controllers;

use Illuminate\Http\Request;
use Illuminate\Http\JsonResponse;
use App\Models\Comment;

class CommentController extends Controller
{
    public function index(): JsonResponse
    {
        return response()->json(Comment::all());
    }

    public function show(Comment $comment): JsonResponse
    {
        return response()->json($comment);
    }

    public function store(Request $request): JsonResponse
    {
        $comment = Comment::create($request->validate([
            'post_id' => 'required|exists:posts,id',
            'body' => 'required|string',
        ]));
        return response()->json($comment, 201);
    }

    public function update(Request $request, Comment $comment): JsonResponse
    {
        $comment->update($request->validate([
            'body' => 'required|string',
        ]));
        return response()->json($comment);
    }

    public function destroy(Comment $comment): JsonResponse
    {
        $comment->delete();
        return response()->json(null, 204);
    }
}
