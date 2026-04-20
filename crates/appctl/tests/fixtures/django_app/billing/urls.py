from django.urls import path
from rest_framework.routers import DefaultRouter

router = DefaultRouter()
router.register(r"parcels", None, basename="parcel")
router.register(r"customers", None, basename="customer")

urlpatterns = [
    path("api/", None),
]
