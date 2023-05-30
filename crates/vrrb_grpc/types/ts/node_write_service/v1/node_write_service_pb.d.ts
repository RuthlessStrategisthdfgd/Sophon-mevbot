// @generated by protoc-gen-es v1.2.0
// @generated from file node_write_service/v1/node_write_service.proto (package node_write_service.v1, syntax proto3)
/* eslint-disable */
// @ts-nocheck

import type { BinaryReadOptions, FieldList, JsonReadOptions, JsonValue, PartialMessage, PlainMessage } from "@bufbuild/protobuf";
import { Message, proto3 } from "@bufbuild/protobuf";

/**
 * @generated from message node_write_service.v1.CreateTransactionRequest
 */
export declare class CreateTransactionRequest extends Message<CreateTransactionRequest> {
  /**
   * @generated from field: int64 timestamp = 1;
   */
  timestamp: bigint;

  /**
   * @generated from field: string sender_address = 2;
   */
  senderAddress: string;

  /**
   * @generated from field: string sender_public_key = 3;
   */
  senderPublicKey: string;

  /**
   * @generated from field: string receiver_address = 4;
   */
  receiverAddress: string;

  /**
   * @generated from field: node_write_service.v1.Token token = 5;
   */
  token?: Token;

  /**
   * @generated from field: uint64 amount = 6;
   */
  amount: bigint;

  /**
   * @generated from field: string signature = 7;
   */
  signature: string;

  /**
   * @generated from field: map<string, bool> validators = 8;
   */
  validators: { [key: string]: boolean };

  /**
   * @generated from field: uint64 nonce = 9;
   */
  nonce: bigint;

  constructor(data?: PartialMessage<CreateTransactionRequest>);

  static readonly runtime: typeof proto3;
  static readonly typeName = "node_write_service.v1.CreateTransactionRequest";
  static readonly fields: FieldList;

  static fromBinary(bytes: Uint8Array, options?: Partial<BinaryReadOptions>): CreateTransactionRequest;

  static fromJson(jsonValue: JsonValue, options?: Partial<JsonReadOptions>): CreateTransactionRequest;

  static fromJsonString(jsonString: string, options?: Partial<JsonReadOptions>): CreateTransactionRequest;

  static equals(a: CreateTransactionRequest | PlainMessage<CreateTransactionRequest> | undefined, b: CreateTransactionRequest | PlainMessage<CreateTransactionRequest> | undefined): boolean;
}

/**
 * @generated from message node_write_service.v1.TransactionRecord
 */
export declare class TransactionRecord extends Message<TransactionRecord> {
  /**
   * @generated from field: string id = 1;
   */
  id: string;

  /**
   * @generated from field: int64 timestamp = 2;
   */
  timestamp: bigint;

  /**
   * @generated from field: string sender_address = 3;
   */
  senderAddress: string;

  /**
   * @generated from field: string sender_public_key = 4;
   */
  senderPublicKey: string;

  /**
   * @generated from field: string receiver_address = 5;
   */
  receiverAddress: string;

  /**
   * @generated from field: node_write_service.v1.Token token = 6;
   */
  token?: Token;

  /**
   * @generated from field: uint64 amount = 7;
   */
  amount: bigint;

  /**
   * @generated from field: string signature = 8;
   */
  signature: string;

  /**
   * @generated from field: map<string, bool> validators = 9;
   */
  validators: { [key: string]: boolean };

  /**
   * @generated from field: uint64 nonce = 10;
   */
  nonce: bigint;

  constructor(data?: PartialMessage<TransactionRecord>);

  static readonly runtime: typeof proto3;
  static readonly typeName = "node_write_service.v1.TransactionRecord";
  static readonly fields: FieldList;

  static fromBinary(bytes: Uint8Array, options?: Partial<BinaryReadOptions>): TransactionRecord;

  static fromJson(jsonValue: JsonValue, options?: Partial<JsonReadOptions>): TransactionRecord;

  static fromJsonString(jsonString: string, options?: Partial<JsonReadOptions>): TransactionRecord;

  static equals(a: TransactionRecord | PlainMessage<TransactionRecord> | undefined, b: TransactionRecord | PlainMessage<TransactionRecord> | undefined): boolean;
}

/**
 * @generated from message node_write_service.v1.Token
 */
export declare class Token extends Message<Token> {
  /**
   * @generated from field: string name = 1;
   */
  name: string;

  /**
   * @generated from field: string symbol = 2;
   */
  symbol: string;

  /**
   * @generated from field: uint32 decimals = 3;
   */
  decimals: number;

  constructor(data?: PartialMessage<Token>);

  static readonly runtime: typeof proto3;
  static readonly typeName = "node_write_service.v1.Token";
  static readonly fields: FieldList;

  static fromBinary(bytes: Uint8Array, options?: Partial<BinaryReadOptions>): Token;

  static fromJson(jsonValue: JsonValue, options?: Partial<JsonReadOptions>): Token;

  static fromJsonString(jsonString: string, options?: Partial<JsonReadOptions>): Token;

  static equals(a: Token | PlainMessage<Token> | undefined, b: Token | PlainMessage<Token> | undefined): boolean;
}

