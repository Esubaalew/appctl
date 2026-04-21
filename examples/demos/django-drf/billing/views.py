from rest_framework import viewsets
from .models import Parcel, Customer
from .serializers import ParcelSerializer, CustomerSerializer


class ParcelViewSet(viewsets.ModelViewSet):
    queryset = Parcel.objects.all()
    serializer_class = ParcelSerializer


class CustomerViewSet(viewsets.ModelViewSet):
    queryset = Customer.objects.all()
    serializer_class = CustomerSerializer
