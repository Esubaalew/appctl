<?php

use Illuminate\Support\Facades\Route;
use App\Http\Controllers\PostController;

Route::apiResource('posts', PostController::class);
Route::apiResource('comments', \App\Http\Controllers\CommentController::class);
