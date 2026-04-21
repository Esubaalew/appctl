<?php

namespace App\Http\Controllers;

use Illuminate\Http\Request;
use Illuminate\Http\JsonResponse;
use App\Models\Post;

class PostController extends Controller
{
    public function index(): JsonResponse
    {
        return response()->json(Post::all());
    }

    public function show(Post $post): JsonResponse
    {
        return response()->json($post);
    }

    public function store(Request $request): JsonResponse
    {
        $post = Post::create($request->validate([
            'title' => 'required|string|max:255',
            'body' => 'nullable|string',
            'published' => 'boolean',
            'tags' => 'nullable|array',
        ]));
        return response()->json($post, 201);
    }

    public function update(Request $request, Post $post): JsonResponse
    {
        $post->update($request->validate([
            'title' => 'sometimes|string|max:255',
            'body' => 'nullable|string',
            'published' => 'boolean',
            'tags' => 'nullable|array',
        ]));
        return response()->json($post);
    }

    public function destroy(Post $post): JsonResponse
    {
        $post->delete();
        return response()->json(null, 204);
    }
}
