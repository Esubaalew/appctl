from rest_framework import serializers
from .models import Parcel, Customer


class ParcelSerializer(serializers.ModelSerializer):
    class Meta:
        model = Parcel
        fields = "__all__"


class CustomerSerializer(serializers.ModelSerializer):
    class Meta:
        model = Customer
        fields = "__all__"
