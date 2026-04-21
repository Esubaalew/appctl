from django.urls import path, include
from rest_framework.routers import DefaultRouter
from .views import ParcelViewSet, CustomerViewSet

router = DefaultRouter()
router.register(r"parcels", ParcelViewSet)
router.register(r"customers", CustomerViewSet)

urlpatterns = [
    path("", include(router.urls)),
]
